//! The command line, exercised as a subprocess.
//!
//! Running the real binary is the only way to check the things that only exist
//! at that level: exit codes, which stream output lands on, and the refusal to
//! approve a policy without a terminal.

use std::path::Path;
use std::process::{Command, Output};

use safescope::cli::approve;
use safescope::registry;
use safescope::store::DATA_DIR_ENV;
use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_safescope");

const POLICY: &str = "\
schema_version = 1

[scope]
allow = [\"src/auth/**\"]
deny = [\"**/.env\"]
";

/// Serialises tests that set the process-wide data directory.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn run(data: &Path, workspace: &Path, args: &[&str]) -> Output {
    Command::new(BINARY)
        .env(DATA_DIR_ENV, data)
        // Assertions read English, so the subprocess is pinned to it.
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

fn dirs() -> (TempDir, TempDir) {
    (
        TempDir::new().expect("data"),
        TempDir::new().expect("workspace"),
    )
}

/// Registers the workspace and approves `POLICY` without going through the
/// terminal prompt, which a test cannot answer.
fn approved(data: &Path, workspace: &Path) {
    unsafe { std::env::set_var(DATA_DIR_ENV, data) };
    let registration = registry::load(workspace).expect("registered");
    std::fs::write(registration.policy_path(), POLICY).expect("write policy");
    let policy = approve::check_policy(POLICY).expect("valid policy");
    approve::perform(&registration, policy, POLICY).expect("approve");
}

#[test]
fn init_registers_the_project() {
    let _guard = env_lock();
    let (data, workspace) = dirs();

    let output = run(data.path(), workspace.path(), &["init"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("Registered"));
    assert!(workspace.path().join(".safescope/policy.toml").is_file());
}

#[test]
fn init_twice_is_refused() {
    let _guard = env_lock();
    let (data, workspace) = dirs();

    run(data.path(), workspace.path(), &["init"]);
    let output = run(data.path(), workspace.path(), &["init"]);
    assert_eq!(output.status.code(), Some(1), "a refusal, not a crash");
    assert!(stderr(&output).contains("WORKSPACE_ALREADY_REGISTERED"));
}

#[test]
fn approving_without_a_terminal_is_refused() {
    // The gate that matters. It is not a strong boundary — an agent with shell
    // access can run this binary — but it stops an approval that nobody typed.
    let _guard = env_lock();
    let (data, workspace) = dirs();

    run(data.path(), workspace.path(), &["init"]);
    std::fs::write(workspace.path().join(".safescope/policy.toml"), POLICY).expect("write");

    let output = run(data.path(), workspace.path(), &["policy", "approve"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("APPROVAL_NEEDS_TTY"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn an_invalid_policy_is_refused_before_the_terminal_check() {
    // Reporting the terminal requirement first would make a person open a shell
    // only to be told their policy was wrong all along.
    let _guard = env_lock();
    let (data, workspace) = dirs();

    run(data.path(), workspace.path(), &["init"]);
    let output = run(data.path(), workspace.path(), &["policy", "approve"]);
    assert!(
        stderr(&output).contains("POLICY_INVALID"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn check_needs_an_approved_policy() {
    let _guard = env_lock();
    let (data, workspace) = dirs();

    run(data.path(), workspace.path(), &["init"]);
    let output = run(
        data.path(),
        workspace.path(),
        &["check", "src/a.rs", "--op", "replace"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("NO_APPROVED_POLICY"));
}

#[test]
fn check_explains_an_allowed_path_and_exits_zero() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    run(data.path(), workspace.path(), &["init"]);
    approved(data.path(), workspace.path());

    let output = run(
        data.path(),
        workspace.path(),
        &["check", "src/auth/Login.java", "--op", "replace"],
    );
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert!(text.contains("Allowed"), "{text}");
    assert!(
        text.contains("src/auth/**"),
        "the matching rule is named: {text}"
    );
    assert!(
        text.contains("policy.toml:4"),
        "the rule's line is shown: {text}"
    );
}

#[test]
fn check_exits_one_when_a_path_is_refused() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    run(data.path(), workspace.path(), &["init"]);
    approved(data.path(), workspace.path());

    let output = run(
        data.path(),
        workspace.path(),
        &["check", "src/other/X.java", "--op", "replace"],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "a script can read the answer"
    );
    let text = stdout(&output);
    assert!(text.contains("SCOPE_DENIED"), "{text}");
    assert!(
        text.contains("expansion"),
        "an expansion is possible here: {text}"
    );
}

#[test]
fn check_reports_a_deny_rule_as_unopenable() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    run(data.path(), workspace.path(), &["init"]);
    approved(data.path(), workspace.path());

    let output = run(
        data.path(),
        workspace.path(),
        &["check", "src/.env", "--op", "replace"],
    );
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(text.contains("**/.env"), "{text}");
    assert!(text.contains("cannot be opened by an approval"), "{text}");
}

#[test]
fn check_refuses_a_traversal_argument() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    run(data.path(), workspace.path(), &["init"]);
    approved(data.path(), workspace.path());

    let output = run(
        data.path(),
        workspace.path(),
        &["check", "../escape", "--op", "replace"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("INVALID_PATH"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn check_refuses_an_unknown_operation() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    run(data.path(), workspace.path(), &["init"]);
    approved(data.path(), workspace.path());

    let output = run(
        data.path(),
        workspace.path(),
        &["check", "src/a.rs", "--op", "delete"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("create, replace, move or trash"));
}

#[test]
fn policy_show_reports_unapproved_edits() {
    // The case this exists for: someone edits the policy, does not approve it,
    // and believes the change is in force.
    let _guard = env_lock();
    let (data, workspace) = dirs();
    run(data.path(), workspace.path(), &["init"]);
    approved(data.path(), workspace.path());

    let clean = run(data.path(), workspace.path(), &["policy", "show"]);
    assert!(!stdout(&clean).contains("unapproved"), "{}", stdout(&clean));

    std::fs::write(
        workspace.path().join(".safescope/policy.toml"),
        format!("{POLICY}\n# edited\n"),
    )
    .expect("edit policy");

    let edited = run(data.path(), workspace.path(), &["policy", "show"]);
    assert!(
        stdout(&edited).contains("unapproved edits"),
        "{}",
        stdout(&edited)
    );
}

#[test]
fn output_follows_the_language_setting() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    run(data.path(), workspace.path(), &["init"]);
    approved(data.path(), workspace.path());

    let output = Command::new(BINARY)
        .env(DATA_DIR_ENV, data.path())
        .env("SAFESCOPE_LANG", "ko")
        .arg("--workspace")
        .arg(workspace.path())
        .args(["check", "src/auth/Login.java", "--op", "replace"])
        .output()
        .expect("run");

    let text = stdout(&output);
    assert!(text.contains("허용"), "{text}");
    assert!(text.contains("평가 과정"), "{text}");
}

#[test]
fn approving_a_scope_expansion_without_a_terminal_is_refused() {
    // The terminal half of the two-tier approval. It is not a strong boundary —
    // an agent with shell access can run this binary — but it stops an approval
    // that nobody typed.
    let _guard = env_lock();
    let (data, workspace) = dirs();
    run(data.path(), workspace.path(), &["init"]);
    approved(data.path(), workspace.path());

    let output = run(
        data.path(),
        workspace.path(),
        &["approve", "docs/Notes.md", "--op", "create"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("APPROVAL_NEEDS_TTY"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn approving_an_unknown_operation_is_refused() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    run(data.path(), workspace.path(), &["init"]);
    approved(data.path(), workspace.path());

    let output = run(
        data.path(),
        workspace.path(),
        &["approve", "docs/Notes.md", "--op", "delete"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("create, replace, move or trash"));
}

#[test]
fn approving_a_traversing_path_is_refused() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    run(data.path(), workspace.path(), &["init"]);
    approved(data.path(), workspace.path());

    let output = run(data.path(), workspace.path(), &["approve", "../escape"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("INVALID_PATH"));
}
