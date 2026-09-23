//! What happens to recovery data once nobody needs it.
//!
//! The snapshot store counts against a workspace-wide storage limit, so with
//! nothing ever removed a workspace that has done enough work refuses *every*
//! change, in every task, permanently. `retain_closed_task_days` was written
//! into the policy schema, given a default and validated — and then read by
//! nothing at all, which is what these pin shut.

use std::collections::HashSet;
use std::fs;
use std::time::{Duration, SystemTime};

use safescope::cli::approve;
use safescope::hash::ContentHash;
use safescope::paths::RelPath;
use safescope::planner::ChangeRequest;
use safescope::registry;
use safescope::retention;
use safescope::session::WriteSession;
use safescope::store::DATA_DIR_ENV;
use safescope::store::content::ContentStore;
use tempfile::TempDir;

/// Serialises the tests, because `SAFESCOPE_DATA_DIR` is process-wide.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

const POLICY: &str = "\
schema_version = 1

[scope]
allow = [\"src/**\"]

[recovery]
retain_closed_task_days = 14
";

fn workspace(policy: &str) -> (TempDir, TempDir) {
    let data = TempDir::new().expect("data");
    let root = TempDir::new().expect("workspace");
    unsafe { std::env::set_var(DATA_DIR_ENV, data.path()) };

    fs::create_dir_all(root.path().join("src")).expect("mkdir");
    fs::write(root.path().join("src/main.rs"), "before\n").expect("seed");
    let registration = registry::init(root.path()).expect("init");
    fs::write(registration.policy_path(), policy).expect("policy");
    let approved = approve::check_policy(policy).expect("valid");
    approve::perform(&registration, approved, policy).expect("approve");

    (data, root)
}

/// Makes one destructive change, so a snapshot exists to reclaim.
fn change(session: &mut WriteSession, contents: &str) {
    let plan = session
        .plan(&ChangeRequest::Replace {
            path: RelPath::parse("src/main.rs").expect("path"),
            contents: contents.as_bytes().to_vec(),
        })
        .expect("plan");
    session.apply(&plan, None).expect("apply");
}

fn snapshot_bytes(root: &std::path::Path) -> u64 {
    let paths = registry::load(root)
        .expect("registration")
        .state_paths()
        .expect("paths");
    ContentStore::snapshots(&paths)
        .usage_bytes()
        .expect("usage")
}

#[test]
fn a_finished_tasks_snapshots_go_once_the_window_has_passed() {
    let _guard = env_lock();
    let (_data, root) = workspace(POLICY);

    let mut session = WriteSession::open(root.path()).expect("open");
    change(&mut session, "after\n");
    assert!(snapshot_bytes(root.path()) > 0, "a snapshot was taken");
    session.finish().expect("finish");

    // Swept as of a date past the window rather than by waiting a fortnight.
    let paths = registry::load(root.path())
        .expect("registration")
        .state_paths()
        .expect("paths");
    let journal = safescope::journal::Journal::open(&paths).expect("journal");
    let snapshots = ContentStore::snapshots(&paths);
    let settings = safescope::policy::RecoverySettings {
        conflict_action: safescope::policy::ConflictAction::Stop,
        retain_closed_task_days: 14,
        protect_incomplete_tasks: true,
    };

    let later = SystemTime::now() + Duration::from_secs(15 * 24 * 60 * 60);
    let reclaimed =
        retention::sweep(&journal, &snapshots, &settings, session.task(), later).expect("sweep");

    assert!(!reclaimed.is_empty(), "nothing was reclaimed");
    assert_eq!(snapshot_bytes(root.path()), 0, "the store is still full");
}

#[test]
fn the_current_tasks_snapshots_stay_however_old_the_clock_says_they_are() {
    // Undo walks back through the current task. Reclaiming its snapshots
    // because a clock moved would take away the thing undo runs on.
    let _guard = env_lock();
    let (_data, root) = workspace(POLICY);

    let mut session = WriteSession::open(root.path()).expect("open");
    change(&mut session, "after\n");
    let before = snapshot_bytes(root.path());

    let paths = registry::load(root.path())
        .expect("registration")
        .state_paths()
        .expect("paths");
    let journal = safescope::journal::Journal::open(&paths).expect("journal");
    let settings = safescope::policy::RecoverySettings {
        conflict_action: safescope::policy::ConflictAction::Stop,
        retain_closed_task_days: 0,
        protect_incomplete_tasks: true,
    };

    let reclaimed = retention::sweep(
        &journal,
        &ContentStore::snapshots(&paths),
        &settings,
        session.task(),
        SystemTime::now() + Duration::from_secs(365 * 24 * 60 * 60),
    )
    .expect("sweep");

    assert!(reclaimed.is_empty(), "the current task lost a snapshot");
    assert_eq!(snapshot_bytes(root.path()), before);
}

#[test]
fn finishing_a_task_reclaims_nothing_while_the_window_holds() {
    // The default window is a fortnight, so a task finished a moment ago keeps
    // everything — undo after `finish` is the ordinary case.
    let _guard = env_lock();
    let (_data, root) = workspace(POLICY);

    let mut session = WriteSession::open(root.path()).expect("open");
    change(&mut session, "after\n");
    let before = snapshot_bytes(root.path());
    session.finish().expect("finish");

    assert_eq!(
        snapshot_bytes(root.path()),
        before,
        "finishing threw away recovery data the window still covers"
    );
}

#[test]
fn a_blob_two_records_share_survives_while_either_is_kept() {
    // Content addressing means one file can be the snapshot of several
    // operations. Removing it with the first would take it from the second.
    let root = TempDir::new().expect("store");
    let store = ContentStore::at(root.path().join("blobs"));
    let shared = store.store(b"same contents").expect("store");
    let lonely = store.store(b"other contents").expect("store");

    let keep: HashSet<ContentHash> = [shared].into_iter().collect();
    let reclaimed = store.retain(&keep).expect("retain");

    assert_eq!(reclaimed.blobs, 1, "exactly the unreferenced one");
    assert!(
        store.verify(shared).expect("verify"),
        "the shared blob went"
    );
    assert!(!store.contains(lonely));
}

#[test]
fn a_sweep_that_keeps_everything_removes_nothing() {
    let root = TempDir::new().expect("store");
    let store = ContentStore::at(root.path().join("blobs"));
    let one = store.store(b"a").expect("store");
    let two = store.store(b"b").expect("store");

    let keep: HashSet<ContentHash> = [one, two].into_iter().collect();
    assert!(store.retain(&keep).expect("retain").is_empty());
    assert!(store.verify(one).expect("verify"));
    assert!(store.verify(two).expect("verify"));
}

#[test]
fn a_file_this_store_did_not_write_is_left_alone() {
    // A sweep that deleted anything it did not recognise would make the store
    // unsafe to put anything beside.
    let root = TempDir::new().expect("store");
    let directory = root.path().join("blobs");
    let store = ContentStore::at(directory.clone());
    store.store(b"a").expect("store");

    let intruder = directory.join("ab").join("not-a-hash.txt");
    fs::create_dir_all(intruder.parent().expect("parent")).expect("mkdir");
    fs::write(&intruder, "somebody else's").expect("write");

    store.retain(&HashSet::new()).expect("retain");
    assert!(intruder.is_file(), "a foreign file was removed");
}
