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

#[test]
fn a_free_workspace_can_be_locked() {
    let directory = TempDir::new().expect("temp dir");
    let lock = WorkspaceLock::acquire_at(&lock_path(&directory)).expect("acquire");
    assert!(lock.path().is_file(), "the lock file is created");
}

#[test]
fn a_second_holder_is_refused_rather_than_made_to_wait() {
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
    let directory = TempDir::new().expect("temp dir");
    let path = lock_path(&directory);

    let held = WorkspaceLock::acquire_at(&path).expect("first");
    drop(held);

    WorkspaceLock::acquire_at(&path).expect("the lock was released");
}

#[test]
fn separate_workspaces_do_not_block_each_other() {
    let first = TempDir::new().expect("first");
    let second = TempDir::new().expect("second");

    let _one = WorkspaceLock::acquire_at(&lock_path(&first)).expect("first");
    let _two = WorkspaceLock::acquire_at(&lock_path(&second)).expect("second");
}

#[test]
fn the_lock_file_outlives_the_lock() {
    // Unlinking it would let a second process create a fresh file and lock
    // that instead, leaving both believing they held the workspace.
    let directory = TempDir::new().expect("temp dir");
    let path = lock_path(&directory);

    let held = WorkspaceLock::acquire_at(&path).expect("acquire");
    drop(held);
    assert!(path.is_file());
}

#[test]
fn the_lock_is_released_when_the_holder_dies() {
    // A crash must not leave a workspace permanently unusable. The kernel drops
    // the lock with the process, which is why this is an flock rather than a
    // file somebody has to remember to clean up.
    //
    // The first version of this test shelled out to flock(1), which does not
    // exist on macOS — so it passed while proving nothing. The child is now
    // checked to be genuinely holding the lock before anything is concluded
    // from letting it go.
    let directory = TempDir::new().expect("temp dir");
    let path = lock_path(&directory);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");

    let holder = Command::new("python3")
        .arg("-c")
        .arg(
            "import fcntl, sys, time\n\
             handle = open(sys.argv[1], 'w')\n\
             fcntl.flock(handle, fcntl.LOCK_EX)\n\
             print('held', flush=True)\n\
             time.sleep(60)\n",
        )
        .arg(&path)
        .stdout(Stdio::piped())
        .spawn();

    let Ok(mut holder) = holder else {
        eprintln!("skipped: python3 is needed to hold a lock from another process");
        return;
    };

    // Wait for the child to say it has the lock, rather than guessing at a delay.
    let mut announcement = String::new();
    BufReader::new(holder.stdout.take().expect("stdout"))
        .read_line(&mut announcement)
        .expect("read");
    assert_eq!(announcement.trim(), "held");

    assert_eq!(
        WorkspaceLock::acquire_at(&path).unwrap_err().code(),
        ErrorCode::WorkspaceBusy,
        "the child really is holding it, so the rest of this test means something"
    );

    holder.kill().expect("kill");
    holder.wait().expect("reap");

    WorkspaceLock::acquire_at(&path).expect("the dead holder's lock is gone");
}
