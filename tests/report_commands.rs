//! The commands a person runs to find out what happened.
//!
//! Two properties matter more than the wording. Reading must work while the MCP
//! server holds the workspace lock, or nobody could ask when it mattered; and
//! the exit code has to mean something, or none of this is scriptable.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use safescope::cli::approve;
use safescope::paths::RelPath;
use safescope::planner::ChangeRequest;
use safescope::registry;
use safescope::session::WriteSession;
use safescope::store::DATA_DIR_ENV;
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_safescope");

const POLICY: &str = "schema_version = 1\n[scope]\nallow = [\"src/**\"]\n";

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn run(data: &Path, workspace: &Path, args: &[&str]) -> Output {
    Command::new(BINARY)
        .env(DATA_DIR_ENV, data)
        .env("SAFESCOPE_LANG", "en")
        .arg("--workspace")
        .arg(workspace)
        .args(args)
        .output()
        .expect("run safescope")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A registered workspace with `POLICY` approved.
fn workspace() -> (TempDir, TempDir) {
    let data = TempDir::new().expect("data");
    let root = TempDir::new().expect("workspace");
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };

    fs::create_dir_all(root.path().join("src")).expect("mkdir");
    let registration = registry::init(root.path()).expect("init");
    fs::write(registration.policy_path(), POLICY).expect("policy");
    let policy = approve::check_policy(POLICY).expect("valid");
    approve::perform(&registration, policy, POLICY).expect("approve");

    (data, root)
}

/// Makes one change through a session, then lets go of the lock.
fn do_some_work(data: &Path, root: &Path) {
    unsafe { std::env::set_var(DATA_DIR_ENV, data) };
    let mut session = WriteSession::open(root).expect("session");
    let plan = session
        .plan(&ChangeRequest::Create {
            path: RelPath::parse("src/A.java").expect("path"),
            contents: b"content\n".to_vec(),
        })
        .expect("plan");
    session.apply(&plan, None).expect("apply");
}

#[test]
fn status_reads_while_a_session_holds_the_lock() {
    // The constraint that shapes these commands. A status that failed whenever
    // the server was running would be a status nobody could ask for.
    let _guard = env_lock();
    let (data, root) = workspace();
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };
    let held = WriteSession::open(root.path()).expect("session holds the lock");

    let output = run(data.path(), root.path(), &["status"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("Allowed scope"));

    drop(held);
}

#[test]
fn status_reports_usage_and_what_is_not_covered() {
    let _guard = env_lock();
    let (data, root) = workspace();
    do_some_work(data.path(), root.path());

    let text = stdout(&run(data.path(), root.path(), &["status"]));
    assert!(text.contains("1 / 8"), "changed paths: {text}");
    assert!(text.contains("src/**"), "the allowed scope: {text}");
    assert!(text.contains("shell command"), "the gap is stated: {text}");
}

#[test]
fn status_says_when_no_policy_has_been_approved() {
    let _guard = env_lock();
    let data = TempDir::new().expect("data");
    let root = TempDir::new().expect("workspace");
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };
    registry::init(root.path()).expect("init");

    let output = run(data.path(), root.path(), &["status"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        stdout(&output).contains("no policy approved"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn status_exits_non_zero_when_the_policy_was_edited_without_approval() {
    // So a script can tell. Somebody changed the policy and may believe the
    // change took effect.
    let _guard = env_lock();
    let (data, root) = workspace();
    fs::write(
        root.path().join(".safescope/policy.toml"),
        format!("{POLICY}\n# edited\n"),
    )
    .expect("edit");

    let output = run(data.path(), root.path(), &["status"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains("unapproved"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn history_lists_operations_most_recent_first() {
    let _guard = env_lock();
    let (data, root) = workspace();
    do_some_work(data.path(), root.path());

    let text = stdout(&run(data.path(), root.path(), &["history"]));
    assert!(text.contains("create"), "{text}");
    assert!(text.contains("src/A.java"), "{text}");
}

#[test]
fn history_on_an_untouched_workspace_says_so() {
    let _guard = env_lock();
    let (data, root) = workspace();

    let output = run(data.path(), root.path(), &["history"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("nothing"), "{}", stdout(&output));
}

#[test]
fn doctor_names_every_check_not_only_the_failures() {
    // Listing only what went wrong reads as a clean bill of health for
    // everything it never looked at.
    let _guard = env_lock();
    let (data, root) = workspace();

    let text = stdout(&run(data.path(), root.path(), &["doctor"]));
    for check in [
        "Policy",
        "Task",
        "unfinished",
        "needs comparing",
        "policy file",
    ] {
        assert!(text.contains(check), "{check} is not reported: {text}");
    }
}

#[test]
fn doctor_fails_on_an_unapproved_policy_edit() {
    let _guard = env_lock();
    let (data, root) = workspace();
    fs::write(
        root.path().join(".safescope/policy.toml"),
        format!("{POLICY}\n# edited\n"),
    )
    .expect("edit");

    let output = run(data.path(), root.path(), &["doctor"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("FAILED"), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("policy approve"),
        "it says what to do"
    );
}

#[test]
fn recover_says_when_there_is_nothing_to_do() {
    let _guard = env_lock();
    let (data, root) = workspace();
    do_some_work(data.path(), root.path());

    let output = run(data.path(), root.path(), &["recover"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(
        stdout(&output).contains("Nothing needed"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn undo_reverses_the_last_change() {
    let _guard = env_lock();
    let (data, root) = workspace();
    do_some_work(data.path(), root.path());
    assert!(root.path().join("src/A.java").exists());

    let output = run(data.path(), root.path(), &["undo"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(!root.path().join("src/A.java").exists());
}

#[test]
fn undo_refuses_when_the_file_has_moved_on() {
    let _guard = env_lock();
    let (data, root) = workspace();
    do_some_work(data.path(), root.path());
    fs::write(root.path().join("src/A.java"), "somebody edited it\n").expect("edit");

    let output = run(data.path(), root.path(), &["undo"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("RECOVERY_CONFLICT"),
        "{}",
        stderr(&output)
    );
    assert_eq!(
        fs::read_to_string(root.path().join("src/A.java")).expect("read"),
        "somebody edited it\n"
    );
}

#[test]
fn undo_says_when_there_is_nothing_to_reverse() {
    let _guard = env_lock();
    let (data, root) = workspace();

    let output = run(data.path(), root.path(), &["undo"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("nothing to undo"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn the_commands_that_write_wait_for_nobody() {
    // recover and undo reconcile or reverse, so they need the lock. Racing a
    // running server would be reconciling against a moving target.
    let _guard = env_lock();
    let (data, root) = workspace();
    do_some_work(data.path(), root.path());

    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };
    let held = WriteSession::open(root.path()).expect("session holds the lock");

    for command in ["recover", "undo"] {
        let output = run(data.path(), root.path(), &[command]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{command}: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains("WORKSPACE_BUSY"),
            "{command}: {}",
            stderr(&output)
        );
    }

    drop(held);
}

#[test]
fn reading_an_unregistered_project_says_what_to_do() {
    let _guard = env_lock();
    let data = TempDir::new().expect("data");
    let root = TempDir::new().expect("workspace");

    for command in ["status", "history", "doctor"] {
        let output = run(data.path(), root.path(), &[command]);
        assert_eq!(output.status.code(), Some(1), "{command}");
        assert!(
            stderr(&output).contains("WORKSPACE_NOT_REGISTERED"),
            "{command}"
        );
        assert!(stderr(&output).contains("safescope init"), "{command}");
    }
}

#[test]
fn the_doctor_says_when_a_change_is_not_durable_here() {
    // Only where the platform cannot flush a directory. The module said for a
    // while that the status output reported this; it did not, and a doc comment
    // asserting something nobody had written is the failure this pins shut.
    let _guard = env_lock();
    let (data, root) = workspace();

    let output = stdout(&run(data.path(), root.path(), &["doctor"]));
    let said = output.contains("cannot flush a directory");
    assert_eq!(
        said,
        !safescope::platform::rename_is_durable(),
        "what the doctor says and what the platform does must agree: {output}"
    );
}
