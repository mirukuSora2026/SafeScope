//! The PreToolUse hook, driven the way Claude Code drives it.
//!
//! The wire format belongs to the host, so these run the real binary with real
//! JSON on stdin and read the exact field names back. Testing the decision
//! function alone would confirm the author's idea of the protocol rather than
//! the protocol.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use safescope::cli::approve;
use safescope::registry;
use safescope::store::DATA_DIR_ENV;
use serde_json::{Value, json};
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_safescope");

const POLICY: &str = "\
schema_version = 1

[scope]
allow = [\"src/auth/**\"]
deny = [\"**/.env\"]
";

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Runs `safescope hook` with `payload` on stdin.
fn hook(data: &Path, payload: &Value) -> Output {
    let mut child = Command::new(BINARY)
        .env(DATA_DIR_ENV, data)
        .env("SAFESCOPE_LANG", "en")
        .arg("hook")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn safescope hook");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(payload.to_string().as_bytes())
        .expect("write payload");
    child.wait_with_output().expect("wait")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A registered workspace with POLICY approved.
fn approved_workspace() -> (TempDir, TempDir) {
    let data = TempDir::new().expect("data");
    let workspace = TempDir::new().expect("workspace");
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };

    let registration = registry::init(workspace.path()).expect("init");
    std::fs::write(registration.policy_path(), POLICY).expect("write policy");
    let policy = approve::check_policy(POLICY).expect("valid");
    approve::perform(&registration, policy, POLICY).expect("approve");

    std::fs::create_dir_all(workspace.path().join("src/auth")).expect("create dirs");
    (data, workspace)
}

fn write_payload(workspace: &Path, relative: &str) -> Value {
    json!({
        "session_id": "s",
        "hook_event_name": "PreToolUse",
        "cwd": workspace.to_string_lossy(),
        "tool_name": "Write",
        "tool_input": { "file_path": workspace.join(relative).to_string_lossy() },
    })
}

#[test]
fn stays_silent_for_a_path_in_scope() {
    // Silence rather than an explicit allow: an allow would quietly waive
    // whatever else the user had configured in their own permission rules.
    let _guard = env_lock();
    let (data, workspace) = approved_workspace();

    let output = hook(
        data.path(),
        &write_payload(workspace.path(), "src/auth/Login.java"),
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).trim().is_empty(), "{}", stdout(&output));
}

#[test]
fn denies_a_path_outside_the_allowed_scope() {
    let _guard = env_lock();
    let (data, workspace) = approved_workspace();

    let output = hook(
        data.path(),
        &write_payload(workspace.path(), "src/other/X.java"),
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "the decision carries the refusal, not the code"
    );

    let decision: Value = serde_json::from_str(stdout(&output).trim()).expect("valid JSON");
    let specific = &decision["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], "PreToolUse");
    assert_eq!(specific["permissionDecision"], "deny");

    let reason = specific["permissionDecisionReason"]
        .as_str()
        .expect("a reason");
    assert!(reason.contains("src/other/X.java"), "{reason}");
    // The hint is what stops Claude retrying the same edit.
    assert!(reason.contains("expansion"), "{reason}");
}

#[test]
fn denies_a_path_a_deny_rule_covers() {
    let _guard = env_lock();
    let (data, workspace) = approved_workspace();

    let output = hook(data.path(), &write_payload(workspace.path(), "src/.env"));
    let decision: Value = serde_json::from_str(stdout(&output).trim()).expect("valid JSON");
    let reason = decision["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .expect("a reason");
    assert!(reason.contains("**/.env"), "{reason}");
}

#[test]
fn denies_a_protected_path() {
    let _guard = env_lock();
    let (data, workspace) = approved_workspace();

    let output = hook(data.path(), &write_payload(workspace.path(), ".git/config"));
    let decision: Value = serde_json::from_str(stdout(&output).trim()).expect("valid JSON");
    assert_eq!(decision["hookSpecificOutput"]["permissionDecision"], "deny");
}

#[test]
fn says_nothing_about_a_shell_command() {
    // A shell command cannot be read reliably — sed -i, a redirect, a script.
    // A check that looks like protection without being it is worse than none,
    // and the status output is where that gap gets reported instead.
    let _guard = env_lock();
    let (data, workspace) = approved_workspace();

    let payload = json!({
        "hook_event_name": "PreToolUse",
        "cwd": workspace.path().to_string_lossy(),
        "tool_name": "Bash",
        "tool_input": { "command": "sed -i '' s/a/b/ src/other/X.java" },
    });
    let output = hook(data.path(), &payload);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).trim().is_empty());
}

#[test]
fn says_nothing_about_a_read_only_tool() {
    let _guard = env_lock();
    let (data, workspace) = approved_workspace();

    let payload = json!({
        "hook_event_name": "PreToolUse",
        "cwd": workspace.path().to_string_lossy(),
        "tool_name": "Read",
        "tool_input": { "file_path": workspace.path().join("src/other/X.java").to_string_lossy() },
    });
    assert!(stdout(&hook(data.path(), &payload)).trim().is_empty());
}

#[test]
fn says_nothing_outside_a_workspace() {
    let _guard = env_lock();
    let data = TempDir::new().expect("data");
    let elsewhere = TempDir::new().expect("elsewhere");

    let payload = json!({
        "hook_event_name": "PreToolUse",
        "cwd": elsewhere.path().to_string_lossy(),
        "tool_name": "Write",
        "tool_input": { "file_path": elsewhere.path().join("a.rs").to_string_lossy() },
    });
    let output = hook(data.path(), &payload);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).trim().is_empty());
}

#[test]
fn says_nothing_about_a_path_outside_the_workspace_root() {
    // Claude may be editing a file elsewhere while cwd is a workspace.
    let _guard = env_lock();
    let (data, workspace) = approved_workspace();

    let payload = json!({
        "hook_event_name": "PreToolUse",
        "cwd": workspace.path().to_string_lossy(),
        "tool_name": "Write",
        "tool_input": { "file_path": "/tmp/somewhere-else.rs" },
    });
    assert!(stdout(&hook(data.path(), &payload)).trim().is_empty());
}

#[test]
fn denies_when_the_project_has_no_approved_policy() {
    let _guard = env_lock();
    let data = TempDir::new().expect("data");
    let workspace = TempDir::new().expect("workspace");
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };
    registry::init(workspace.path()).expect("init");

    let output = hook(
        data.path(),
        &write_payload(workspace.path(), "src/auth/Login.java"),
    );
    let decision: Value = serde_json::from_str(stdout(&output).trim()).expect("valid JSON");
    assert_eq!(decision["hookSpecificOutput"]["permissionDecision"], "deny");
    let reason = decision["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .expect("a reason");
    assert!(reason.contains("policy approve"), "{reason}");
}

#[test]
fn a_malformed_payload_does_not_block_anything() {
    // The host's format will grow. A hook that fell over on an unexpected
    // payload would disable itself at the worst possible moment.
    let _guard = env_lock();
    let data = TempDir::new().expect("data");

    let mut child = Command::new(BINARY)
        .env(DATA_DIR_ENV, data.path())
        .arg("hook")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"not json at all")
        .expect("write");
    let output = child.wait_with_output().expect("wait");

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).trim().is_empty());
}

#[test]
fn an_unknown_field_in_the_payload_is_tolerated() {
    let _guard = env_lock();
    let (data, workspace) = approved_workspace();

    let mut payload = write_payload(workspace.path(), "src/other/X.java");
    payload["some_future_field"] = json!({ "added": "later" });

    let output = hook(data.path(), &payload);
    let decision: Value = serde_json::from_str(stdout(&output).trim()).expect("still decides");
    assert_eq!(decision["hookSpecificOutput"]["permissionDecision"], "deny");
}

#[test]
fn never_exits_two() {
    // Exit 2 blocks unconditionally and the reason would come from stderr
    // instead of the decision. Refusals are expressed as decisions so the
    // explanation reaches Claude.
    let _guard = env_lock();
    let (data, workspace) = approved_workspace();

    for relative in [
        "src/auth/Login.java",
        "src/other/X.java",
        "src/.env",
        ".git/config",
    ] {
        let output = hook(data.path(), &write_payload(workspace.path(), relative));
        assert_eq!(output.status.code(), Some(0), "{relative}");
    }
}
