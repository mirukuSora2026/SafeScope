//! Registering a project as a SafeScope workspace.

use std::fs;

use safescope::error::ErrorCode;
use safescope::policy::NormalizedPolicy;
use safescope::registry::{self, CONFIG_DIR, ID_FILE, POLICY_FILE};
use safescope::store::DATA_DIR_ENV;
use tempfile::TempDir;

/// Serialises the tests, because `SAFESCOPE_DATA_DIR` is process-wide.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A temporary project with engine state pointed somewhere else entirely.
fn project() -> (TempDir, TempDir) {
    let data = TempDir::new().expect("data dir");
    let root = TempDir::new().expect("project");
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };
    (data, root)
}

#[test]
fn init_writes_an_identity_and_a_starter_policy() {
    let _guard = env_lock();
    let (_data, root) = project();

    let registration = registry::init(root.path()).expect("init");
    assert!(root.path().join(CONFIG_DIR).join(ID_FILE).is_file());
    assert!(registration.policy_path().is_file());
    assert_eq!(registration.root(), root.path());
}

#[test]
fn the_starter_policy_permits_nothing() {
    // A default that permitted something would mean nobody reads the policy.
    let _guard = env_lock();
    let (_data, root) = project();

    let registration = registry::init(root.path()).expect("init");
    let text = registration.read_policy_text().expect("read");
    let policy = NormalizedPolicy::from_text(&text).expect("parses");
    assert!(policy.allow.is_empty());
    assert!(
        !policy.deny.is_empty(),
        "the standard refusals are still there"
    );
}

#[test]
fn the_identity_survives_a_reload() {
    let _guard = env_lock();
    let (_data, root) = project();

    let first = registry::init(root.path()).expect("init");
    let again = registry::load(root.path()).expect("load");
    assert_eq!(first.id(), again.id());
}

#[test]
fn registering_twice_is_refused() {
    let _guard = env_lock();
    let (_data, root) = project();

    registry::init(root.path()).expect("init");
    let error = registry::init(root.path()).unwrap_err();
    assert_eq!(error.code(), ErrorCode::WorkspaceAlreadyRegistered);
}

#[test]
fn loading_an_unregistered_project_says_what_to_do() {
    let _guard = env_lock();
    let (_data, root) = project();

    let error = registry::load(root.path()).unwrap_err();
    assert_eq!(error.code(), ErrorCode::WorkspaceNotRegistered);
    assert!(
        error.report().hint.is_some(),
        "the caller is told to run init"
    );
}

#[test]
fn a_corrupted_identity_is_reported_rather_than_replaced() {
    // Issuing a new identity would orphan every snapshot and journal record
    // belonging to the old one.
    let _guard = env_lock();
    let (_data, root) = project();

    registry::init(root.path()).expect("init");
    fs::write(root.path().join(CONFIG_DIR).join(ID_FILE), "not a uuid").expect("corrupt");

    let error = registry::load(root.path()).unwrap_err();
    assert_eq!(error.code(), ErrorCode::WorkspaceNotRegistered);
}

#[test]
fn init_does_not_overwrite_a_policy_written_beforehand() {
    let _guard = env_lock();
    let (_data, root) = project();

    let config = root.path().join(CONFIG_DIR);
    fs::create_dir_all(&config).expect("create config dir");
    let mine = "schema_version = 1\n[scope]\nallow = [\"src/**\"]\n";
    fs::write(config.join(POLICY_FILE), mine).expect("write policy");

    let registration = registry::init(root.path()).expect("init");
    assert_eq!(registration.read_policy_text().unwrap(), mine);
}

#[test]
fn state_lives_outside_the_project() {
    let _guard = env_lock();
    let (data, root) = project();

    let registration = registry::init(root.path()).expect("init");
    let paths = registration.state_paths().expect("state paths");
    assert!(paths.root().starts_with(data.path()));
    assert!(!paths.root().starts_with(root.path()));
    assert!(
        paths.approved_policy().is_dir(),
        "directories are created by init"
    );
    assert!(paths.snapshots().is_dir());
}

#[test]
fn a_project_is_not_registered_when_its_state_would_be_unusable() {
    // Checked before anything is written, so a project is never left half
    // registered because its state directory turned out to be unusable.
    let _guard = env_lock();
    let root = TempDir::new().expect("project");
    unsafe { std::env::set_var(DATA_DIR_ENV, root.path().join("inside")) };

    let error = registry::init(root.path()).unwrap_err();
    assert_eq!(error.code(), ErrorCode::WorkspaceStateUnusable);
    assert!(
        !root.path().join(CONFIG_DIR).join(ID_FILE).exists(),
        "nothing should have been written"
    );
}
