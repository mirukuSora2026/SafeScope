//! A writing session.
//!
//! The session is where the single-writer claim becomes true rather than
//! assumed, and where a task's lifetime stops depending on how many times the
//! client was opened.

use std::fs;
use std::path::Path;

use safescope::cli::approve;
use safescope::error::ErrorCode;
use safescope::paths::RelPath;
use safescope::planner::ChangeRequest;
use safescope::registry;
use safescope::session::WriteSession;
use safescope::store::DATA_DIR_ENV;
use tempfile::TempDir;

const POLICY: &str = "\
schema_version = 1

[scope]
allow = [\"src/**\"]
";

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn path(text: &str) -> RelPath {
    RelPath::parse(text).unwrap()
}

/// A registered workspace with `POLICY` approved and a `src/` directory.
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

fn read(root: &Path, relative: &str) -> String {
    fs::read_to_string(root.join(relative)).expect("read")
}

#[test]
fn a_session_can_plan_and_apply() {
    let _guard = env_lock();
    let (_data, root) = workspace();
    let mut session = WriteSession::open(root.path()).expect("open");

    let plan = session
        .plan(&ChangeRequest::Create {
            path: path("src/A.java"),
            contents: b"content\n".to_vec(),
        })
        .expect("plan");
    session.apply(&plan, None).expect("apply");

    assert_eq!(read(root.path(), "src/A.java"), "content\n");
}

#[test]
fn a_second_session_cannot_open_while_the_first_lives() {
    // The single-writer claim. Without it, a budget check and the reservation it
    // leads to could be interleaved with somebody else's.
    let _guard = env_lock();
    let (_data, root) = workspace();

    let first = WriteSession::open(root.path()).expect("first");
    let error = WriteSession::open(root.path()).unwrap_err();
    assert_eq!(error.code(), ErrorCode::WorkspaceBusy);

    drop(first);
    WriteSession::open(root.path()).expect("the lock was released");
}

#[test]
fn a_task_survives_the_session_that_started_it() {
    // Reconnecting a client must not hand somebody a fresh budget: the limit is
    // about the work, not about how many times the tool was opened.
    let _guard = env_lock();
    let (_data, root) = workspace();

    let task = {
        let mut session = WriteSession::open(root.path()).expect("open");
        let plan = session
            .plan(&ChangeRequest::Create {
                path: path("src/A.java"),
                contents: b"x\n".to_vec(),
            })
            .expect("plan");
        session.apply(&plan, None).expect("apply");
        session.task()
    };

    let session = WriteSession::open(root.path()).expect("reopen");
    assert_eq!(session.task(), task);
    assert_eq!(session.status().expect("status").usage.operations, 1);
}

#[test]
fn finishing_a_task_starts_a_fresh_budget() {
    let _guard = env_lock();
    let (_data, root) = workspace();
    let mut session = WriteSession::open(root.path()).expect("open");

    let plan = session
        .plan(&ChangeRequest::Create {
            path: path("src/A.java"),
            contents: b"x\n".to_vec(),
        })
        .expect("plan");
    session.apply(&plan, None).expect("apply");
    assert_eq!(session.status().expect("status").usage.operations, 1);

    let before = session.task();
    session.finish().expect("finish");
    assert_ne!(session.task(), before);
    assert_eq!(session.status().expect("status").usage.operations, 0);
}

#[test]
fn a_session_will_not_open_without_an_approved_policy() {
    // An unapproved workspace permits nothing, and saying so once is more useful
    // than refusing every operation one at a time.
    let _guard = env_lock();
    let data = TempDir::new().expect("data");
    let root = TempDir::new().expect("workspace");
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };
    registry::init(root.path()).expect("init");

    let error = WriteSession::open(root.path()).unwrap_err();
    assert_eq!(error.code(), ErrorCode::NoApprovedPolicy);
    assert!(error.report().hint.is_some());
}

#[test]
fn a_session_will_not_open_on_an_unregistered_project() {
    let _guard = env_lock();
    let data = TempDir::new().expect("data");
    let root = TempDir::new().expect("workspace");
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };

    let error = WriteSession::open(root.path()).unwrap_err();
    assert_eq!(error.code(), ErrorCode::WorkspaceNotRegistered);
}

#[test]
fn a_session_can_undo_its_own_work() {
    let _guard = env_lock();
    let (_data, root) = workspace();
    let mut session = WriteSession::open(root.path()).expect("open");

    let plan = session
        .plan(&ChangeRequest::Create {
            path: path("src/A.java"),
            contents: b"x\n".to_vec(),
        })
        .expect("plan");
    session.apply(&plan, None).expect("apply");

    let undo = session.prepare_undo().expect("prepare undo");
    session.apply_undo(&undo).expect("undo");
    assert!(!root.path().join("src/A.java").exists());
}

#[test]
fn status_reports_what_the_session_knows() {
    let _guard = env_lock();
    let (_data, root) = workspace();
    let mut session = WriteSession::open(root.path()).expect("open");

    let plan = session
        .plan(&ChangeRequest::Create {
            path: path("src/A.java"),
            contents: b"x\n".to_vec(),
        })
        .expect("plan");
    session.apply(&plan, None).expect("apply");

    let status = session.status().expect("status");
    assert_eq!(status.usage.paths(), 1);
    assert_eq!(status.usage.operations, 1);
    assert_eq!(status.allowed.len(), 1);
    assert_eq!(status.unsettled, 0);
    assert_eq!(status.needs_attention, 0);
    assert!(!status.unapproved_policy_edits);
    assert!(status.last_operation.is_some());
}

#[test]
fn status_reports_an_unapproved_policy_edit() {
    // Someone changes the policy, does not approve it, and believes the change
    // is in force. The status output is where that gets caught.
    let _guard = env_lock();
    let (_data, root) = workspace();
    let session = WriteSession::open(root.path()).expect("open");
    assert!(!session.status().expect("status").unapproved_policy_edits);

    fs::write(
        root.path().join(".safescope/policy.toml"),
        format!("{POLICY}\n# edited\n"),
    )
    .expect("edit");

    assert!(session.status().expect("status").unapproved_policy_edits);
}

#[test]
fn a_session_has_no_grants_until_one_is_admitted() {
    // A grant that arrived any way other than the approval flow is not evidence
    // of anything (I6).
    let _guard = env_lock();
    let (_data, root) = workspace();
    let session = WriteSession::open(root.path()).expect("open");
    assert!(session.grants().expect("grants").is_empty());
}
