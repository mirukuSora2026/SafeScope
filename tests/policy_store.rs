//! The approved policy store.
//!
//! What these check, above all, is the property the whole approval design rests
//! on: the engine reads the approved snapshot, so editing the policy file grants
//! nothing until someone approves it.

use std::fs;

use safescope::error::ErrorCode;
use safescope::ids::WorkspaceId;
use safescope::policy::{NormalizedPolicy, PolicyVersion};
use safescope::store::policy_store::PolicyStore;
use safescope::store::{DATA_DIR_ENV, StatePaths};
use tempfile::TempDir;

const POLICY_V1: &str = "schema_version = 1\n[scope]\nallow = [\"src/**\"]\n";
const POLICY_V2: &str = "schema_version = 1\n[scope]\nallow = [\"src/**\", \"docs/**\"]\n";

/// A store rooted in a temporary data directory.
///
/// `SAFESCOPE_DATA_DIR` is set for the process; the tests that use it are
/// serialised behind one lock because environment variables are process-wide.
fn store(data: &TempDir, workspace: &TempDir) -> (StatePaths, PolicyStore) {
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };
    let paths = StatePaths::for_workspace(WorkspaceId::new(), workspace.path())
        .expect("state lives outside the workspace");
    paths.create().expect("create state directories");
    let store = PolicyStore::new(&paths);
    (paths, store)
}

fn dirs() -> (TempDir, TempDir) {
    (
        TempDir::new().expect("data dir"),
        TempDir::new().expect("workspace"),
    )
}

#[test]
fn an_unapproved_store_has_no_policy() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    let (_paths, store) = store(&data, &workspace);

    assert!(store.current().expect("read").is_none());
    assert!(store.current_version().expect("read").is_none());
}

#[test]
fn approving_assigns_the_first_version() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    let (_paths, store) = store(&data, &workspace);

    let policy = NormalizedPolicy::from_text(POLICY_V1).unwrap();
    let approved = store.approve(policy, POLICY_V1).expect("approve");

    assert_eq!(approved.version, PolicyVersion::FIRST);
    assert_eq!(store.current_version().unwrap(), Some(PolicyVersion::FIRST));
}

#[test]
fn each_approval_increments_the_version() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    let (_paths, store) = store(&data, &workspace);

    store
        .approve(NormalizedPolicy::from_text(POLICY_V1).unwrap(), POLICY_V1)
        .expect("first");
    let second = store
        .approve(NormalizedPolicy::from_text(POLICY_V2).unwrap(), POLICY_V2)
        .expect("second");

    assert_eq!(second.version, PolicyVersion::FIRST.next());
    // Earlier versions stay readable: a plan made against one has to be able to
    // say what it was made against.
    let first = store
        .read(PolicyVersion::FIRST)
        .expect("first is still there");
    assert_eq!(first.policy.allow.len(), 1);
    assert_eq!(second.policy.allow.len(), 2);
}

#[test]
fn an_approved_policy_round_trips() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    let (_paths, store) = store(&data, &workspace);

    let policy = NormalizedPolicy::from_text(POLICY_V1).unwrap();
    let approved = store.approve(policy.clone(), POLICY_V1).expect("approve");
    let read_back = store.current().expect("read").expect("present");

    assert_eq!(read_back, approved);
    assert_eq!(read_back.policy, policy);
}

#[test]
fn editing_the_policy_file_does_not_change_what_is_approved() {
    // The property the whole approval design rests on.
    let _guard = env_lock();
    let (data, workspace) = dirs();
    let (_paths, store) = store(&data, &workspace);

    store
        .approve(NormalizedPolicy::from_text(POLICY_V1).unwrap(), POLICY_V1)
        .expect("approve");

    let in_force = store.current().unwrap().expect("present");
    assert_eq!(in_force.policy.allow.len(), 1, "still the approved version");
    assert!(
        !store.matches_source(&in_force, POLICY_V2),
        "the store can tell the file has unapproved edits"
    );
    assert!(store.matches_source(&in_force, POLICY_V1));
}

#[test]
fn a_corrupted_pointer_is_reported_rather_than_guessed_at() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    let (_paths, store) = store(&data, &workspace);

    store
        .approve(NormalizedPolicy::from_text(POLICY_V1).unwrap(), POLICY_V1)
        .expect("approve");
    fs::write(store.directory().join("current"), "not a number").expect("corrupt");

    let error = store.current_version().unwrap_err();
    assert_eq!(error.code(), ErrorCode::JournalFailed);
}

#[test]
fn a_corrupted_snapshot_is_reported_rather_than_guessed_at() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    let (_paths, store) = store(&data, &workspace);

    store
        .approve(NormalizedPolicy::from_text(POLICY_V1).unwrap(), POLICY_V1)
        .expect("approve");
    fs::write(store.directory().join("000001.json"), "{ truncated").expect("corrupt");

    let error = store.current().unwrap_err();
    assert_eq!(error.code(), ErrorCode::JournalFailed);
}

#[test]
fn state_is_refused_inside_the_workspace_it_protects() {
    // Snapshots within the tree they protect would be at risk from the very
    // operations they exist to undo.
    let _guard = env_lock();
    let workspace = TempDir::new().expect("workspace");
    unsafe { std::env::set_var(DATA_DIR_ENV, workspace.path().join(".safescope-state")) };

    let error = StatePaths::for_workspace(WorkspaceId::new(), workspace.path())
        .expect_err("registration must be refused");
    assert_eq!(error.code(), ErrorCode::WorkspaceStateUnusable);
    assert!(error.is_denial());
}

#[test]
fn writing_leaves_no_temporary_behind() {
    let _guard = env_lock();
    let (data, workspace) = dirs();
    let (_paths, store) = store(&data, &workspace);

    store
        .approve(NormalizedPolicy::from_text(POLICY_V1).unwrap(), POLICY_V1)
        .expect("approve");

    let leftovers: Vec<_> = fs::read_dir(store.directory())
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".sfs-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}

/// Serialises the tests, because `SAFESCOPE_DATA_DIR` is process-wide.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
