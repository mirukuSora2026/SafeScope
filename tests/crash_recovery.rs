//! What the engine concludes after being killed mid-operation.
//!
//! These spawn a real process and abort it at a chosen point, because the whole
//! question is what survives when the process does not. Anything short of that
//! would be testing the author's model of a crash rather than a crash.
//!
//! Requires the `fault-injection` feature:
//!
//! ```text
//! cargo test --features fault-injection --test crash_recovery
//! ```
#![cfg(feature = "fault-injection")]

use std::fs;
use std::path::Path;
use std::process::Command;

use safescope::journal::{Journal, Stage};
use safescope::path_guard::Workspace;
use safescope::recovery::{Recovery, RecoveryReport};
use safescope::registry;
use safescope::store::DATA_DIR_ENV;
use tempfile::TempDir;

const HARNESS: &str = env!("CARGO_BIN_EXE_sfs-crash-harness");
const ORIGINAL: &str = "the original contents\n";
const REPLACEMENT: &str = "the replacement contents\n";
const CREATED: &str = "freshly created\n";

/// A workspace with an approved policy and `src/target.txt` in place.
struct Crashed {
    data: TempDir,
    root: TempDir,
}

impl Crashed {
    fn prepared() -> Self {
        let data = TempDir::new().expect("data");
        let root = TempDir::new().expect("workspace");
        let workspace = root.path().join("ws");

        let status = Command::new(HARNESS)
            .env(DATA_DIR_ENV, data.path())
            .args(["setup", &workspace.to_string_lossy()])
            .status()
            .expect("run setup");
        assert!(status.success(), "setup failed");

        Self { data, root }
    }

    fn workspace(&self) -> std::path::PathBuf {
        self.root.path().join("ws")
    }

    /// Runs one operation, aborting at `fault`. Returns nothing: the point is
    /// that the process died.
    fn crash_during(&self, operation: &str, fault: &str) {
        let output = Command::new(HARNESS)
            .env(DATA_DIR_ENV, self.data.path())
            .env("SAFESCOPE_FAULT", fault)
            .args(["apply", &self.workspace().to_string_lossy(), operation])
            .output()
            .expect("run apply");

        assert_eq!(
            output.status.code(),
            None,
            "the harness should have been killed by a signal, not exited: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Runs recovery the way a later session would.
    /// Runs `body` with this workspace's state directory in the environment.
    ///
    /// `SAFESCOPE_DATA_DIR` is process-wide while these tests run in parallel,
    /// so it is set and used under one lock. Without that, a test reads another
    /// test's state directory — and only sometimes, which is the worst way for
    /// a test to be wrong.
    fn with_state<T>(&self, body: impl FnOnce() -> T) -> T {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe { std::env::set_var(DATA_DIR_ENV, self.data.path()) };
        body()
    }

    /// Runs recovery the way a later session would.
    fn recover(&self) -> RecoveryReport {
        self.with_state(|| {
            let registration = registry::load(&self.workspace()).expect("registered");
            let paths = registration.state_paths().expect("state paths");
            let workspace = Workspace::open(&self.workspace()).expect("open");
            let mut journal = Journal::open(&paths).expect("journal");

            Recovery {
                workspace: &workspace,
                journal: &mut journal,
            }
            .run()
            .expect("recover")
        })
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.workspace().join(relative)).expect("read")
    }

    fn exists(&self, relative: &str) -> bool {
        self.workspace().join(relative).exists()
    }

    fn temporaries_in(&self, directory: &str) -> Vec<String> {
        let path: &Path = &self.workspace().join(directory);
        fs::read_dir(path)
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".sfs-tmp-"))
            .collect()
    }
}

fn sole_conclusion(report: &RecoveryReport) -> Stage {
    assert_eq!(report.conclusions.len(), 1, "{report:?}");
    report.conclusions[0].now
}

#[test]
fn killed_after_the_plan_was_recorded_it_never_started() {
    // The applying record is committed before the first filesystem call, so an
    // operation still at Prepared cannot have touched anything.
    let crashed = Crashed::prepared();
    crashed.crash_during("replace", "after_prepared_record");

    assert_eq!(sole_conclusion(&crashed.recover()), Stage::Aborted);
    assert_eq!(crashed.read("src/target.txt"), ORIGINAL);
}

#[test]
fn killed_after_the_applying_record_it_did_not_run() {
    let crashed = Crashed::prepared();
    crashed.crash_during("replace", "after_applying_record");

    assert_eq!(sole_conclusion(&crashed.recover()), Stage::Aborted);
    assert_eq!(crashed.read("src/target.txt"), ORIGINAL);
}

#[test]
fn killed_between_writing_the_temporary_and_renaming_it_nothing_changed() {
    let crashed = Crashed::prepared();
    crashed.crash_during("replace", "after_temporary_write_before_rename");

    assert!(
        !crashed.temporaries_in("src").is_empty(),
        "the crash left one behind"
    );

    let report = crashed.recover();
    assert_eq!(sole_conclusion(&report), Stage::Aborted);
    assert_eq!(crashed.read("src/target.txt"), ORIGINAL);
    assert!(
        !report.temporaries_removed.is_empty(),
        "recovery cleaned it up"
    );
    assert!(crashed.temporaries_in("src").is_empty());
}

#[test]
fn killed_after_the_rename_but_before_the_commit_it_did_run() {
    // The hardest window: the file changed and nothing recorded it. The plan's
    // after-hash is what makes this decidable rather than a guess.
    let crashed = Crashed::prepared();
    crashed.crash_during("replace", "after_rename_before_commit");

    let report = crashed.recover();
    assert_eq!(sole_conclusion(&report), Stage::Committed);
    assert_eq!(crashed.read("src/target.txt"), REPLACEMENT);
}

#[test]
fn a_create_killed_before_the_rename_left_no_file() {
    let crashed = Crashed::prepared();
    crashed.crash_during("create", "after_temporary_write_before_rename");

    assert_eq!(sole_conclusion(&crashed.recover()), Stage::Aborted);
    assert!(!crashed.exists("src/created.txt"));
}

#[test]
fn a_create_killed_after_the_rename_is_recognised_as_done() {
    let crashed = Crashed::prepared();
    crashed.crash_during("create", "after_rename_before_commit");

    assert_eq!(sole_conclusion(&crashed.recover()), Stage::Committed);
    assert_eq!(crashed.read("src/created.txt"), CREATED);
}

#[test]
fn a_trash_killed_after_the_snapshot_left_the_file_alone() {
    let crashed = Crashed::prepared();
    crashed.crash_during("trash", "after_snapshot_before_unlink");

    assert_eq!(sole_conclusion(&crashed.recover()), Stage::Aborted);
    assert_eq!(crashed.read("src/target.txt"), ORIGINAL);
}

#[test]
fn a_trash_killed_after_the_unlink_is_recognised_as_done() {
    let crashed = Crashed::prepared();
    crashed.crash_during("trash", "after_rename_before_commit");

    assert_eq!(sole_conclusion(&crashed.recover()), Stage::Committed);
    assert!(!crashed.exists("src/target.txt"));
}

#[test]
fn a_move_killed_after_the_rename_is_recognised_as_done() {
    let crashed = Crashed::prepared();
    crashed.crash_during("move", "after_rename_before_commit");

    let report = crashed.recover();
    assert_eq!(sole_conclusion(&report), Stage::Committed);
    assert!(!crashed.exists("src/target.txt"));
    assert_eq!(crashed.read("src/moved.txt"), ORIGINAL);
}

#[test]
fn an_edit_made_after_the_crash_is_reported_as_a_conflict() {
    // Neither the before-state nor the after-state. The engine cannot say what
    // happened, and guessing here is how somebody's work gets destroyed.
    let crashed = Crashed::prepared();
    crashed.crash_during("replace", "after_applying_record");
    fs::write(
        crashed.workspace().join("src/target.txt"),
        "somebody edited this\n",
    )
    .expect("edit");

    let report = crashed.recover();
    assert_eq!(sole_conclusion(&report), Stage::Conflict);
    assert_eq!(report.unresolved().count(), 1);
    assert_eq!(
        crashed.read("src/target.txt"),
        "somebody edited this\n",
        "recovery repairs nothing"
    );
}

#[test]
fn recovery_reaches_the_same_answer_twice() {
    let crashed = Crashed::prepared();
    crashed.crash_during("replace", "after_rename_before_commit");

    let first = crashed.recover();
    assert_eq!(sole_conclusion(&first), Stage::Committed);

    // Settled operations are not revisited, so a second pass has nothing to do.
    let second = crashed.recover();
    assert!(second.is_empty(), "{second:?}");
    assert_eq!(crashed.read("src/target.txt"), REPLACEMENT);
}

#[test]
fn a_clean_workspace_needs_no_recovery() {
    let crashed = Crashed::prepared();
    assert!(crashed.recover().is_empty());
}

#[test]
fn the_recovery_data_survives_the_crash() {
    // A crash that lost the snapshot would leave a change that happened and
    // cannot be undone, which is the one outcome the design exists to prevent.
    let crashed = Crashed::prepared();
    crashed.crash_during("replace", "after_rename_before_commit");
    crashed.recover();

    let snapshots = crashed.with_state(|| {
        let registration = registry::load(&crashed.workspace()).expect("registered");
        let paths = registration.state_paths().expect("paths");
        safescope::store::content::ContentStore::snapshots(&paths)
    });
    let hash = safescope::hash::ContentHash::of_bytes(ORIGINAL.as_bytes());

    assert!(snapshots.verify(hash).expect("verify"));
    assert_eq!(snapshots.read(hash).expect("read"), ORIGINAL.as_bytes());
}
