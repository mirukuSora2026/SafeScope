//! Changes made around the engine.
//!
//! Every one of these writes to the workspace the way a shell command would —
//! straight through `std::fs`, with the engine knowing nothing about it — because
//! that is the thing being detected. A test that went through the engine to
//! produce drift would be testing the opposite of what this is for.

use std::fs;
use std::path::Path;

use safescope::cli::approve;
use safescope::drift::{Baseline, Change, survey};
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

/// Serialises the tests, because `SAFESCOPE_DATA_DIR` is process-wide.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn path(text: &str) -> RelPath {
    RelPath::parse(text).unwrap()
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

/// Writes to the workspace the way something outside the engine would.
fn write_around_the_engine(root: &Path, relative: &str, contents: &str) {
    let target = root.join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(target, contents).expect("write");
}

#[test]
fn a_baseline_records_the_files_that_are_there() {
    let root = TempDir::new().expect("workspace");
    fs::create_dir_all(root.path().join("src")).expect("mkdir");
    fs::write(root.path().join("src/a.rs"), "a").expect("write");
    fs::write(root.path().join("b.txt"), "b").expect("write");

    let baseline = Baseline::capture(root.path());
    let paths: Vec<&str> = baseline
        .entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect();
    assert_eq!(paths, vec!["b.txt", "src/a.rs"]);
    assert!(!baseline.truncated);
}

#[test]
fn a_baseline_does_not_descend_into_the_engines_own_directory() {
    // `.safescope` holds the policy and the workspace identity. Counting an
    // approval as drift would report the act of setting SafeScope up as
    // somebody going around it.
    let root = TempDir::new().expect("workspace");
    registry::init(root.path()).expect("init");
    fs::write(root.path().join("kept.txt"), "k").expect("write");

    let baseline = Baseline::capture(root.path());
    assert_eq!(baseline.entries.len(), 1);
    assert_eq!(baseline.entries[0].path.as_str(), "kept.txt");
}

#[test]
fn a_file_written_around_the_engine_is_reported_as_changed() {
    let _guard = env_lock();
    let (_data, root) = workspace();
    // Opening a session starts a task, which is what takes the baseline.
    let session = WriteSession::open(root.path()).expect("open");

    write_around_the_engine(root.path(), "src/main.rs", "fn main() { /* theirs */ }\n");

    let survey = session.drift().expect("survey").expect("a baseline");
    assert_eq!(survey.entries.len(), 1);
    assert_eq!(survey.entries[0].path.as_str(), "src/main.rs");
    assert_eq!(survey.entries[0].change, Change::Modified);
}

#[test]
fn a_file_created_around_the_engine_is_reported_as_added() {
    let _guard = env_lock();
    let (_data, root) = workspace();
    let session = WriteSession::open(root.path()).expect("open");

    write_around_the_engine(root.path(), "src/extra.rs", "// not asked for\n");

    let survey = session.drift().expect("survey").expect("a baseline");
    assert_eq!(survey.entries.len(), 1);
    assert_eq!(survey.entries[0].path.as_str(), "src/extra.rs");
    assert_eq!(survey.entries[0].change, Change::Added);
}

#[test]
fn a_file_deleted_around_the_engine_is_reported_as_removed() {
    let _guard = env_lock();
    let (_data, root) = workspace();
    let session = WriteSession::open(root.path()).expect("open");

    fs::remove_file(root.path().join("src/main.rs")).expect("remove");

    let survey = session.drift().expect("survey").expect("a baseline");
    assert_eq!(survey.entries.len(), 1);
    assert_eq!(survey.entries[0].path.as_str(), "src/main.rs");
    assert_eq!(survey.entries[0].change, Change::Removed);
}

#[test]
fn the_engines_own_change_is_not_drift() {
    // The whole thing is worthless if it cries wolf over SafeScope's own work:
    // a report where every change is listed is a report nobody reads.
    let _guard = env_lock();
    let (_data, root) = workspace();
    let mut session = WriteSession::open(root.path()).expect("open");

    let plan = session
        .plan(&ChangeRequest::Replace {
            path: path("src/main.rs"),
            contents: b"fn main() { /* ours */ }\n".to_vec(),
        })
        .expect("plan");
    session.apply(&plan, None).expect("apply");

    let survey = session.drift().expect("survey").expect("a baseline");
    assert!(
        survey.is_clean(),
        "the engine's own change was reported as drift: {:?}",
        survey.entries
    );
}

#[test]
fn a_change_on_top_of_the_engines_own_change_is_still_drift() {
    // The interesting case: SafeScope wrote the file, then something else
    // rewrote it. Excluding every path the engine ever touched would lose this.
    let _guard = env_lock();
    let (_data, root) = workspace();
    let mut session = WriteSession::open(root.path()).expect("open");

    let plan = session
        .plan(&ChangeRequest::Replace {
            path: path("src/main.rs"),
            contents: b"fn main() { /* ours */ }\n".to_vec(),
        })
        .expect("plan");
    session.apply(&plan, None).expect("apply");
    write_around_the_engine(root.path(), "src/main.rs", "fn main() { /* theirs */ }\n");

    let survey = session.drift().expect("survey").expect("a baseline");
    assert_eq!(survey.entries.len(), 1);
    assert_eq!(survey.entries[0].change, Change::Modified);
}

#[test]
fn accepting_drift_stops_it_being_reported() {
    let _guard = env_lock();
    let (_data, root) = workspace();
    let session = WriteSession::open(root.path()).expect("open");

    write_around_the_engine(root.path(), "src/extra.rs", "// kept on purpose\n");
    assert!(
        !session
            .drift()
            .expect("survey")
            .expect("baseline")
            .is_clean()
    );

    session.accept_drift().expect("accept");
    assert!(
        session
            .drift()
            .expect("survey")
            .expect("baseline")
            .is_clean()
    );
}

#[test]
fn a_workspace_without_a_baseline_says_it_does_not_know() {
    // Not the same as saying nothing drifted, and the difference matters: one
    // is a measurement and the other is an absence of one.
    let root = TempDir::new().expect("workspace");
    let baseline = Baseline::capture(root.path());
    assert!(baseline.entries.is_empty());

    let survey = survey(root.path(), &baseline, &[]);
    assert!(survey.is_clean());
    assert_eq!(survey.scanned, 0);
}

#[test]
fn a_symlink_is_not_followed_to_decide_what_changed() {
    // A link that pointed outside the workspace would otherwise let whatever it
    // aims at decide what the baseline covers.
    #[cfg(unix)]
    {
        let root = TempDir::new().expect("workspace");
        let outside = TempDir::new().expect("outside");
        fs::write(outside.path().join("secret"), "s").expect("write");
        std::os::unix::fs::symlink(outside.path().join("secret"), root.path().join("link"))
            .expect("symlink");

        let baseline = Baseline::capture(root.path());
        assert!(
            baseline.entries.is_empty(),
            "a symlink was followed: {:?}",
            baseline.entries
        );
    }
}
