//! Running a command that cannot write to the workspace.
//!
//! Against the real kernel and a real child process — `sandbox-exec` on macOS,
//! Landlock on Linux. The guard's whole claim is that the kernel refuses the
//! write, so a test that stubbed the sandbox would be testing the claim by
//! assuming it.
//!
//! The same tests run on both, because the claim is the same on both. Only the
//! sentence the kernel is given differs.

#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::fs;
use std::path::Path;
use std::process::Command;

use safescope::cli::approve;
use safescope::guard;
use safescope::mcp::socket::SOCKET_ENV;
use safescope::registry;
use safescope::store::DATA_DIR_ENV;
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_safescope");

const POLICY: &str = "\
schema_version = 1

[scope]
allow = [\"src/**\"]
";

/// Serialises the tests, because `SAFESCOPE_DATA_DIR` is process-wide.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A registered workspace with `POLICY` approved and one file in `src/`.
fn workspace() -> (TempDir, TempDir) {
    let data = TempDir::new().expect("data");
    let root = TempDir::new().expect("workspace");
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };

    fs::create_dir_all(root.path().join("src")).expect("mkdir");
    fs::write(root.path().join("src/main.rs"), "fn main() {}\n").expect("seed");
    let registration = registry::init(root.path()).expect("init");
    fs::write(registration.policy_path(), POLICY).expect("policy");
    let policy = approve::check_policy(POLICY).expect("valid");
    approve::perform(&registration, policy, POLICY).expect("approve");

    (data, root)
}

/// Runs `script` under the guard and returns its output.
fn guarded(data: &Path, root: &Path, script: &str) -> std::process::Output {
    Command::new(BINARY)
        .env(DATA_DIR_ENV, data)
        .env("SAFESCOPE_LANG", "en")
        .arg("--workspace")
        .arg(root)
        .arg("guard")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg(script)
        .output()
        .expect("run the guard")
}

/// Whether this kernel can guard at all.
///
/// A Linux older than Landlock refuses, and these tests would then be asserting
/// that a refusal refused. Skipping says so out loud rather than passing on a
/// technicality.
fn can_guard() -> bool {
    #[cfg(target_os = "linux")]
    {
        if safescope::guard::landlock::abi_version().is_none() {
            eprintln!("skipped: this kernel has no Landlock");
            return false;
        }
    }
    true
}

fn text(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn a_guarded_command_cannot_write_a_file_in_the_workspace() {
    if !can_guard() {
        return;
    }
    let _serialised = env_lock();
    let (data, root) = workspace();

    let output = guarded(
        data.path(),
        root.path(),
        "echo changed > src/main.rs && echo WROTE || echo REFUSED",
    );
    assert!(
        text(&output).contains("REFUSED"),
        "the write was allowed: {}",
        text(&output)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("src/main.rs")).expect("read"),
        "fn main() {}\n",
        "the file changed anyway"
    );
}

#[test]
fn a_guarded_command_cannot_create_a_file_either() {
    // A scope check is about paths that exist in a policy. This is about the
    // workspace, so a new file nobody wrote a rule for is refused too.
    if !can_guard() {
        return;
    }
    let _serialised = env_lock();
    let (data, root) = workspace();

    let output = guarded(
        data.path(),
        root.path(),
        "echo x > src/new.rs && echo WROTE || echo REFUSED",
    );
    assert!(text(&output).contains("REFUSED"), "{}", text(&output));
    assert!(!root.path().join("src/new.rs").exists());
}

#[test]
fn a_guarded_command_cannot_delete_a_file() {
    if !can_guard() {
        return;
    }
    let _serialised = env_lock();
    let (data, root) = workspace();

    guarded(data.path(), root.path(), "rm -f src/main.rs");
    assert!(
        root.path().join("src/main.rs").is_file(),
        "the file was deleted"
    );
}

#[test]
fn a_guarded_command_cannot_reach_around_through_another_program() {
    // The routes an agent actually took when the edit tools were denied were
    // all some way of running a command. Under the guard it does not matter
    // which program runs: the sandbox is inherited by everything started
    // inside it.
    if !can_guard() {
        return;
    }
    let _serialised = env_lock();
    let (data, root) = workspace();

    let output = guarded(
        data.path(),
        root.path(),
        "python3 -c \"open('src/main.rs','w').write('theirs')\" 2>/dev/null \
         && echo WROTE || echo REFUSED",
    );
    assert!(text(&output).contains("REFUSED"), "{}", text(&output));
    assert_eq!(
        fs::read_to_string(root.path().join("src/main.rs")).expect("read"),
        "fn main() {}\n"
    );
}

#[test]
fn a_guarded_command_can_still_read_the_workspace() {
    // Denying reads would make the guard useless: an agent that cannot read the
    // code cannot change it correctly either.
    if !can_guard() {
        return;
    }
    let _serialised = env_lock();
    let (data, root) = workspace();

    let output = guarded(data.path(), root.path(), "cat src/main.rs");
    assert!(
        text(&output).contains("fn main()"),
        "reading was denied: {}",
        text(&output)
    );
}

#[test]
fn a_guarded_command_can_write_outside_the_workspace() {
    // The guard removes one capability, over one subtree. A profile that also
    // broke temporary files would be one people turn off.
    if !can_guard() {
        return;
    }
    let _serialised = env_lock();
    let (data, root) = workspace();
    let elsewhere = TempDir::new().expect("elsewhere");
    let target = elsewhere.path().join("scratch");

    guarded(
        data.path(),
        root.path(),
        &format!("echo fine > {}", target.display()),
    );
    assert_eq!(
        fs::read_to_string(&target).unwrap_or_default().trim(),
        "fine"
    );
}

#[test]
fn the_guard_tells_a_guarded_command_where_the_engine_is() {
    // The engine runs outside the sandbox, so what the host starts has to be a
    // relay to it. This variable is how `safescope mcp` knows which it is.
    if !can_guard() {
        return;
    }
    let _serialised = env_lock();
    let (data, root) = workspace();

    let output = guarded(data.path(), root.path(), &format!("echo ${SOCKET_ENV}"));
    let socket = guard::socket_path(root.path()).expect("socket path");
    assert!(
        text(&output).contains(&socket.display().to_string()),
        "the socket was not named: {}",
        text(&output)
    );
}

#[test]
fn the_guard_reports_what_the_command_returned() {
    if !can_guard() {
        return;
    }
    let _serialised = env_lock();
    let (data, root) = workspace();

    let output = guarded(data.path(), root.path(), "exit 3");
    assert_eq!(
        output.status.code(),
        Some(3),
        "a script in front of this needs the command's own code"
    );
}

#[test]
fn the_socket_does_not_outlive_the_guarded_command() {
    // It lives beside the workspace's state, so a stale one would be picked up
    // by the next run and relayed to nothing.
    if !can_guard() {
        return;
    }
    let _serialised = env_lock();
    let (data, root) = workspace();

    guarded(data.path(), root.path(), "true");
    let socket = guard::socket_path(root.path()).expect("socket path");
    assert!(!socket.exists(), "the socket was left behind");
}

#[test]
fn a_second_guard_on_the_same_workspace_is_refused() {
    // Not two engines on one workspace. Unlinking the socket unconditionally
    // would have cut the first guard off from its own agent, whose later tool
    // calls would then have been answered by the second guard's engine.
    if !can_guard() {
        return;
    }
    let _serialised = env_lock();
    let (data, root) = workspace();

    // The first guard holds its socket for as long as its command runs.
    let mut first = Command::new(BINARY)
        .env(DATA_DIR_ENV, data.path())
        .env("SAFESCOPE_LANG", "en")
        .arg("--workspace")
        .arg(root.path())
        .arg("guard")
        .arg("--")
        .arg("/bin/sh")
        .arg("-c")
        .arg("echo ready; sleep 30")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("first guard");

    // Waited for rather than slept past. And checked: `read_line` returns Ok(0)
    // at end of file, so a first guard that died immediately would have let this
    // sail on and then "prove" that a second one is refused by nothing at all.
    //
    // The reader is held in a binding for the rest of the test rather than left
    // as a temporary. Dropped at the end of this statement it closes the read
    // end of the pipe, the guarded command takes SIGPIPE on its next write, and
    // the guard tidies its socket away — after which a second guard is refused
    // by nothing, which is precisely what this is supposed to catch.
    let mut reader = std::io::BufReader::new(first.stdout.take().expect("stdout"));
    let mut announcement = String::new();
    std::io::BufRead::read_line(&mut reader, &mut announcement).expect("read");
    assert!(
        !announcement.trim().is_empty(),
        "the first guard said nothing, so it is not holding anything"
    );

    let socket = guard::socket_path(root.path()).expect("socket path");
    assert!(
        socket.exists(),
        "the first guard is not listening at {}, so the rest of this test means nothing",
        socket.display()
    );

    let second = guarded(data.path(), root.path(), "echo should not run");

    // Held until here, so the guarded command still has somewhere to write.
    drop(reader);
    first.kill().expect("kill");
    first.wait().expect("reap");

    assert_ne!(
        second.status.code(),
        Some(0),
        "the second guard started anyway: {}",
        text(&second)
    );
    assert!(
        text(&second).to_lowercase().contains("another"),
        "the refusal should say somebody else has it: {}",
        text(&second)
    );
}
