//! One writer at a time.
//!
//! The lock is what makes "single writer" true rather than assumed, which is
//! what the budget's check-then-reserve depends on. It is advisory and scoped to
//! SafeScope: it coordinates SafeScope processes with each other and says
//! nothing about an editor or a shell command touching the same files.

use std::io::{BufRead as _, BufReader};
use std::process::{Command, Stdio};

use safescope::error::ErrorCode;
use safescope::store::lock::WorkspaceLock;
use tempfile::TempDir;

fn lock_path(directory: &TempDir) -> std::path::PathBuf {
    directory.path().join("state").join("lock")
}

/// Serialises these tests, because one of them forks.
///
/// An flock belongs to the open file description, and `fork` duplicates it. A
/// child spawned by one test holds a copy of every descriptor this process has
/// open until it reaches `exec` and CLOEXEC closes them — including a lock
/// another test is in the middle of releasing. That window is short and the
/// resulting failure looked exactly like a lock that was not released, which is
/// the wrong thing to go looking for.
///
/// Serialising is the fix rather than retrying: these tests are about who holds
/// the lock, so a second process holding it by accident is not noise to be
/// tolerated.
fn fork_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn a_free_workspace_can_be_locked() {
    let _serialised = fork_lock();
    let directory = TempDir::new().expect("temp dir");
    let lock = WorkspaceLock::acquire_at(&lock_path(&directory)).expect("acquire");
    assert!(lock.path().is_file(), "the lock file is created");
}

#[test]
fn a_second_holder_is_refused_rather_than_made_to_wait() {
    let _serialised = fork_lock();
    // Waiting would hang on another session that may be sitting at a prompt.
    let directory = TempDir::new().expect("temp dir");
    let path = lock_path(&directory);

    let _held = WorkspaceLock::acquire_at(&path).expect("first");
    let error = WorkspaceLock::acquire_at(&path).unwrap_err();

    assert_eq!(error.code(), ErrorCode::WorkspaceBusy);
    let report = error.report();
    assert!(report.retryable, "the other session will finish eventually");
    assert!(report.hint.is_some());
}

#[test]
fn releasing_lets_the_next_holder_in() {
    let _serialised = fork_lock();
    let directory = TempDir::new().expect("temp dir");
    let path = lock_path(&directory);

    let held = WorkspaceLock::acquire_at(&path).expect("first");
    drop(held);

    WorkspaceLock::acquire_at(&path).expect("the lock was released");
}

#[test]
fn separate_workspaces_do_not_block_each_other() {
    let _serialised = fork_lock();
    let first = TempDir::new().expect("first");
    let second = TempDir::new().expect("second");

    let _one = WorkspaceLock::acquire_at(&lock_path(&first)).expect("first");
    let _two = WorkspaceLock::acquire_at(&lock_path(&second)).expect("second");
}

#[test]
fn the_lock_file_outlives_the_lock() {
    let _serialised = fork_lock();
    // Unlinking it would let a second process create a fresh file and lock
    // that instead, leaving both believing they held the workspace.
    let directory = TempDir::new().expect("temp dir");
    let path = lock_path(&directory);

    let held = WorkspaceLock::acquire_at(&path).expect("acquire");
    drop(held);
    assert!(path.is_file());
}

/// Tells the test binary, started again as a child, to be the lock's holder.
const HOLD_ENV: &str = "SAFESCOPE_TEST_HOLD_LOCK_AT";

/// The other process in the test below, not a test in its own right.
///
/// The test starts this binary again with only this function selected, so the
/// lock is taken by the engine's own code in a process that can be killed —
/// which is what a crash is. Run any other way it does nothing.
#[test]
fn hold_the_lock_until_killed() {
    let Some(path) = std::env::var_os(HOLD_ENV) else {
        return;
    };
    let _held = WorkspaceLock::acquire_at(std::path::Path::new(&path)).expect("hold");
    println!("held");
    std::io::Write::flush(&mut std::io::stdout()).expect("flush");
    std::thread::sleep(std::time::Duration::from_secs(60));
}

#[test]
fn the_lock_is_released_when_the_holder_dies() {
    let _serialised = fork_lock();
    // A crash must not leave a workspace permanently unusable. The kernel drops
    // the lock with the process — an flock on Unix, LockFileEx on Windows —
    // which is why it is not a file somebody has to remember to clean up.
    //
    // The first version of this test shelled out to flock(1), which does not
    // exist on macOS, and the second to Python's fcntl, which does not exist on
    // Windows and was skipped wherever Python was missing — each passed without
    // asking the question somewhere. The holder is now this binary, taking the
    // lock through the engine's own code, and the child is checked to be
    // genuinely holding it before anything is concluded from letting it go.
    let directory = TempDir::new().expect("temp dir");
    let path = lock_path(&directory);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");

    let mut holder = Command::new(std::env::current_exe().expect("this test binary"))
        .args(["hold_the_lock_until_killed", "--exact", "--nocapture"])
        .env(HOLD_ENV, &path)
        .stdout(Stdio::piped())
        .spawn()
        .expect("start the holder");

    // Wait for the child to say it has the lock, rather than guessing at a
    // delay. The harness prints its own lines first.
    let mut stdout = BufReader::new(holder.stdout.take().expect("stdout"));
    let mut line = String::new();
    loop {
        line.clear();
        let read = stdout.read_line(&mut line).expect("read");
        assert!(read > 0, "the holder exited without taking the lock");
        if line.trim() == "held" {
            break;
        }
    }

    assert_eq!(
        WorkspaceLock::acquire_at(&path).unwrap_err().code(),
        ErrorCode::WorkspaceBusy,
        "the child really is holding it, so the rest of this test means something"
    );

    holder.kill().expect("kill");
    holder.wait().expect("reap");

    WorkspaceLock::acquire_at(&path).expect("the dead holder's lock is gone");
}
