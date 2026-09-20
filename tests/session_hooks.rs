//! What SafeScope tells a session at its start and its end.
//!
//! These run the real binary with real payloads, as the host does. The shapes
//! differ between the two events — SessionStart takes
//! `hookSpecificOutput.additionalContext`, Stop honours `systemMessage` — and
//! getting that wrong would mean a message nobody ever sees.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use safescope::cli::approve;
use safescope::domain::Transition;
use safescope::hash::ContentHash;
use safescope::ids::PlanId;
use safescope::journal::{Journal, NewOperation, Stage};
use safescope::paths::RelPath;
use safescope::registry;
use safescope::session::WriteSession;
use safescope::store::DATA_DIR_ENV;
use serde_json::{Value, json};
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_safescope");

const POLICY: &str = "schema_version = 1\n[scope]\nallow = [\"src/**\"]\n";

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A registered workspace with `POLICY` approved.
fn workspace() -> (TempDir, TempDir) {
    let data = TempDir::new().expect("data");
    let root = TempDir::new().expect("workspace");
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };

    std::fs::create_dir_all(root.path().join("src")).expect("mkdir");
    let registration = registry::init(root.path()).expect("init");
    std::fs::write(registration.policy_path(), POLICY).expect("policy");
    let policy = approve::check_policy(POLICY).expect("valid");
    approve::perform(&registration, policy, POLICY).expect("approve");

    (data, root)
}

fn hook(data: &Path, event: &str, workspace: &Path) -> Output {
    let payload = json!({
        "session_id": "s",
        "hook_event_name": event,
        "cwd": workspace.to_string_lossy(),
    });

    let mut child = Command::new(BINARY)
        .env(DATA_DIR_ENV, data)
        .env("SAFESCOPE_LANG", "en")
        .arg("hook")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(payload.to_string().as_bytes())
        .expect("write");
    child.wait_with_output().expect("wait")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Leaves an operation the engine never finished recording.
fn leave_unfinished(data: &Path, root: &Path) {
    unsafe { std::env::set_var(DATA_DIR_ENV, data) };
    let registration = registry::load(root).expect("registered");
    let paths = registration.state_paths().expect("paths");
    let task = safescope::store::task_store::TaskStore::new(&paths)
        .current_or_start()
        .expect("task");

    let mut journal = Journal::open(&paths).expect("journal");
    let record = journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()),
            &Transition::create(
                RelPath::parse("src/A.java").expect("path"),
                ContentHash::of_bytes(b"x"),
                1,
            ),
        )
        .expect("record");
    journal
        .mark(record.id, Stage::Applying, None, None)
        .expect("applying");
}

#[test]
fn a_clean_workspace_is_not_worth_mentioning() {
    // A line that appears every session is a line nobody reads.
    let _guard = env_lock();
    let (data, root) = workspace();

    for event in ["SessionStart", "Stop"] {
        let output = hook(data.path(), event, root.path());
        assert_eq!(output.status.code(), Some(0));
        assert!(stdout(&output).is_empty(), "{event}: {}", stdout(&output));
    }
}

#[test]
fn session_start_reports_work_the_engine_did_not_finish() {
    let _guard = env_lock();
    let (data, root) = workspace();
    leave_unfinished(data.path(), root.path());

    let output = hook(data.path(), "SessionStart", root.path());
    let message: Value = serde_json::from_str(&stdout(&output)).expect("valid JSON");

    assert_eq!(
        message["hookSpecificOutput"]["hookEventName"],
        "SessionStart"
    );
    let context = message["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext");
    assert!(context.contains("recover"), "{context}");
}

#[test]
fn stop_reports_through_the_field_stop_actually_honours() {
    // Stop does not take hookSpecificOutput.additionalContext. Using it would
    // produce a message nobody ever sees.
    let _guard = env_lock();
    let (data, root) = workspace();
    leave_unfinished(data.path(), root.path());

    let output = hook(data.path(), "Stop", root.path());
    let message: Value = serde_json::from_str(&stdout(&output)).expect("valid JSON");

    assert!(message["systemMessage"].is_string(), "{message}");
    assert!(
        message["hookSpecificOutput"].is_null(),
        "Stop ignores this field, so writing it would be a message into the void: {message}"
    );
}

#[test]
fn an_unapproved_policy_edit_is_reported() {
    // Somebody changed the policy and may believe the change took effect.
    let _guard = env_lock();
    let (data, root) = workspace();
    std::fs::write(
        root.path().join(".safescope/policy.toml"),
        format!("{POLICY}\n# edited\n"),
    )
    .expect("edit");

    let output = hook(data.path(), "SessionStart", root.path());
    let message: Value = serde_json::from_str(&stdout(&output)).expect("valid JSON");
    let context = message["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("context");
    assert!(context.contains("policy approve"), "{context}");
}

#[test]
fn it_still_reports_while_a_session_holds_the_lock() {
    // The constraint that shapes this code: a session takes the workspace lock,
    // and the MCP server holds one for its whole life. A hook that opened a
    // session would be a hook that never ran while the server was up.
    let _guard = env_lock();
    let (data, root) = workspace();
    leave_unfinished(data.path(), root.path());

    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };
    let held = WriteSession::open(root.path()).expect("session holds the lock");

    let output = hook(data.path(), "SessionStart", root.path());
    assert_eq!(output.status.code(), Some(0));
    assert!(
        !stdout(&output).is_empty(),
        "it had something to say and said it"
    );

    drop(held);
}

#[test]
fn an_unrecognised_event_is_answered_with_silence() {
    // A guess here becomes a decision.
    let _guard = env_lock();
    let (data, root) = workspace();
    leave_unfinished(data.path(), root.path());

    let output = hook(data.path(), "SomeFutureEvent", root.path());
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).is_empty());
}

#[test]
fn a_project_that_is_not_a_workspace_is_not_commented_on() {
    let _guard = env_lock();
    let data = TempDir::new().expect("data");
    let elsewhere = TempDir::new().expect("elsewhere");

    let output = hook(data.path(), "SessionStart", elsewhere.path());
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).is_empty());
}
