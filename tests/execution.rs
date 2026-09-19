//! The four operations, carried out.
//!
//! Run against a real workspace on disk, because what is being checked is the
//! relationship between the journal, the snapshot store and the files — and that
//! relationship is only real on a filesystem.

mod harness;

use std::fs;

use harness::{Harness, env_lock, path};
use safescope::domain::{FileState, Operation};
use safescope::hash::ContentHash;
use safescope::journal::Stage;
use safescope::planner::ChangeRequest;

#[test]
fn create_writes_the_file_and_records_it() {
    let _guard = env_lock();
    let mut harness = Harness::new();

    let plan = harness
        .plan(ChangeRequest::Create {
            path: path("src/auth/Login.java"),
            contents: b"class Login {}\n".to_vec(),
        })
        .expect("plan");
    let record = harness.apply(&plan, None).expect("apply");

    assert_eq!(harness.read("src/auth/Login.java"), "class Login {}\n");
    assert_eq!(record.stage, Stage::Committed);
    assert_eq!(record.transition.operation(), Operation::Create);

    // The recorded outcome is what was observed afterwards, not what was hoped.
    let observed = record.observed.expect("observed");
    assert_eq!(
        observed[0].state,
        FileState::present(ContentHash::of_bytes(b"class Login {}\n"), 15)
    );
}

#[test]
fn replace_changes_the_file_and_keeps_the_old_contents() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/auth/Login.java", "old\n");
    let old = ContentHash::of_bytes(b"old\n");

    harness
        .run(ChangeRequest::Replace {
            path: path("src/auth/Login.java"),
            contents: b"new\n".to_vec(),
        })
        .expect("replace");

    assert_eq!(harness.read("src/auth/Login.java"), "new\n");
    // I2: what was destroyed is recoverable.
    assert_eq!(harness.snapshots.read(old).expect("snapshot"), b"old\n");
}

#[test]
fn trash_removes_the_file_but_not_its_contents() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/auth/Old.java", "still wanted\n");
    let hash = ContentHash::of_bytes(b"still wanted\n");

    harness
        .run(ChangeRequest::Trash {
            path: path("src/auth/Old.java"),
        })
        .expect("trash");

    assert!(!harness.exists("src/auth/Old.java"));
    assert_eq!(
        harness.snapshots.read(hash).expect("snapshot"),
        b"still wanted\n"
    );
}

#[test]
fn move_renames_the_file() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/auth/Old.java", "content\n");

    harness
        .run(ChangeRequest::Move {
            from: path("src/auth/Old.java"),
            to: path("src/auth/New.java"),
        })
        .expect("move");

    assert!(!harness.exists("src/auth/Old.java"));
    assert_eq!(harness.read("src/auth/New.java"), "content\n");
}

#[test]
fn a_move_stores_no_snapshot() {
    // Undoing a move is the reverse rename; the contents never left.
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/auth/Old.java", "content\n");

    harness
        .run(ChangeRequest::Move {
            from: path("src/auth/Old.java"),
            to: path("src/auth/New.java"),
        })
        .expect("move");

    assert_eq!(harness.snapshots.usage_bytes().expect("usage"), 0);
}

#[test]
fn a_snapshot_exists_before_the_change_is_applied() {
    // I2 is a planning-time guarantee, so the recovery data is already there
    // while the file is still intact.
    let _guard = env_lock();
    let harness = Harness::new();
    harness.write("src/auth/Login.java", "original\n");

    harness
        .plan(ChangeRequest::Replace {
            path: path("src/auth/Login.java"),
            contents: b"new\n".to_vec(),
        })
        .expect("plan");

    let hash = ContentHash::of_bytes(b"original\n");
    assert!(harness.snapshots.verify(hash).expect("verify"));
    assert_eq!(harness.read("src/auth/Login.java"), "original\n");
}

#[test]
fn the_journal_carries_the_whole_sequence() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/auth/A.java", "one\n");

    harness
        .run(ChangeRequest::Replace {
            path: path("src/auth/A.java"),
            contents: b"two\n".to_vec(),
        })
        .expect("replace");
    harness
        .run(ChangeRequest::Move {
            from: path("src/auth/A.java"),
            to: path("src/auth/B.java"),
        })
        .expect("move");
    harness
        .run(ChangeRequest::Trash {
            path: path("src/auth/B.java"),
        })
        .expect("trash");

    let history = harness.journal.history(harness.task).expect("history");
    let operations: Vec<_> = history.iter().map(|r| r.transition.operation()).collect();
    assert_eq!(
        operations,
        [Operation::Replace, Operation::Move, Operation::Trash]
    );
    assert!(
        history
            .iter()
            .all(|record| record.stage == Stage::Committed)
    );
    assert!(
        history
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
}

#[test]
fn an_empty_file_can_be_created_and_replaced() {
    let _guard = env_lock();
    let mut harness = Harness::new();

    harness
        .run(ChangeRequest::Create {
            path: path("src/auth/Empty.java"),
            contents: Vec::new(),
        })
        .expect("create");
    assert_eq!(harness.read("src/auth/Empty.java"), "");

    harness
        .run(ChangeRequest::Replace {
            path: path("src/auth/Empty.java"),
            contents: b"now full\n".to_vec(),
        })
        .expect("replace");
    assert_eq!(harness.read("src/auth/Empty.java"), "now full\n");
}

#[test]
fn applying_leaves_no_temporary_behind() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness
        .run(ChangeRequest::Create {
            path: path("src/auth/Login.java"),
            contents: b"content\n".to_vec(),
        })
        .expect("create");

    let leftovers: Vec<_> = fs::read_dir(harness.root.path().join("src/auth"))
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".sfs-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}
