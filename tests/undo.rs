//! Reversing the last thing the engine did.

mod harness;

use harness::{Harness, env_lock, path};
use safescope::error::ErrorCode;
use safescope::journal::{OperationKind, Stage};
use safescope::planner::ChangeRequest;

#[test]
fn undoing_a_create_removes_the_file() {
    let _guard = env_lock();
    let mut harness = Harness::new();

    harness
        .run(ChangeRequest::Create {
            path: path("src/New.java"),
            contents: b"new\n".to_vec(),
        })
        .expect("create");
    assert!(harness.exists("src/New.java"));

    harness.undo().expect("undo");
    assert!(!harness.exists("src/New.java"));
}

#[test]
fn undoing_a_replace_brings_the_old_contents_back() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/A.java", "original\n");

    harness
        .run(ChangeRequest::Replace {
            path: path("src/A.java"),
            contents: b"changed\n".to_vec(),
        })
        .expect("replace");
    assert_eq!(harness.read("src/A.java"), "changed\n");

    harness.undo().expect("undo");
    assert_eq!(harness.read("src/A.java"), "original\n");
}

#[test]
fn undoing_a_trash_restores_the_file() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/Gone.java", "wanted after all\n");

    harness
        .run(ChangeRequest::Trash {
            path: path("src/Gone.java"),
        })
        .expect("trash");
    assert!(!harness.exists("src/Gone.java"));

    harness.undo().expect("undo");
    assert_eq!(harness.read("src/Gone.java"), "wanted after all\n");
}

#[test]
fn undoing_a_move_puts_the_file_back() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/A.java", "content\n");

    harness
        .run(ChangeRequest::Move {
            from: path("src/A.java"),
            to: path("src/B.java"),
        })
        .expect("move");

    harness.undo().expect("undo");
    assert_eq!(harness.read("src/A.java"), "content\n");
    assert!(!harness.exists("src/B.java"));
}

#[test]
fn undo_walks_backwards_one_operation_at_a_time() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/A.java", "one\n");

    harness
        .run(ChangeRequest::Replace {
            path: path("src/A.java"),
            contents: b"two\n".to_vec(),
        })
        .expect("second");
    harness
        .run(ChangeRequest::Replace {
            path: path("src/A.java"),
            contents: b"three\n".to_vec(),
        })
        .expect("third");

    harness.undo().expect("first undo");
    assert_eq!(harness.read("src/A.java"), "two\n");

    harness.undo().expect("second undo");
    assert_eq!(harness.read("src/A.java"), "one\n");
}

#[test]
fn a_later_edit_stops_an_undo_rather_than_being_overwritten() {
    // The situation undo exists to be careful about: someone worked on the file
    // after the engine did, and reversing would throw that away.
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/A.java", "original\n");

    harness
        .run(ChangeRequest::Replace {
            path: path("src/A.java"),
            contents: b"changed\n".to_vec(),
        })
        .expect("replace");
    harness.write("src/A.java", "then somebody edited it\n");

    let error = harness.undo().unwrap_err();
    assert_eq!(error.code(), ErrorCode::RecoveryConflict);
    assert_eq!(harness.read("src/A.java"), "then somebody edited it\n");

    let report = error.report();
    assert!(report.hint.is_some(), "the person is told how to decide");
    assert!(!report.retryable);
}

#[test]
fn the_recovery_data_survives_a_refused_undo() {
    // The person has to be able to compare, so a conflict must not discard what
    // the comparison needs.
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/A.java", "original\n");

    harness
        .run(ChangeRequest::Replace {
            path: path("src/A.java"),
            contents: b"changed\n".to_vec(),
        })
        .expect("replace");
    harness.write("src/A.java", "edited\n");
    harness.undo().unwrap_err();

    let hash = safescope::hash::ContentHash::of_bytes(b"original\n");
    assert_eq!(
        harness.snapshots.read(hash).expect("snapshot"),
        b"original\n"
    );
}

#[test]
fn there_is_nothing_to_undo_before_anything_is_done() {
    let _guard = env_lock();
    let mut harness = Harness::new();
    let error = harness.undo().unwrap_err();
    assert_eq!(error.code(), ErrorCode::PlanNotFound);
}

#[test]
fn an_undo_is_recorded_as_an_undo() {
    let _guard = env_lock();
    let mut harness = Harness::new();

    harness
        .run(ChangeRequest::Create {
            path: path("src/New.java"),
            contents: b"new\n".to_vec(),
        })
        .expect("create");
    harness.undo().expect("undo");

    let history = harness.journal.history(harness.task).expect("history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].kind, OperationKind::Change);
    assert_eq!(history[1].kind, OperationKind::Undo);
    assert!(
        history
            .iter()
            .all(|record| record.stage == Stage::Committed)
    );
}

#[test]
fn undo_works_even_once_the_budget_is_spent() {
    // Refusing here would leave a person holding exactly the sprawl the limit
    // was meant to prevent.
    let _guard = env_lock();
    let policy = "\
schema_version = 1

[scope]
allow = [\"src/**\"]

[budget]
max_changed_paths = 1
max_operations = 1
";
    let mut harness = Harness::with_policy(policy);

    harness
        .run(ChangeRequest::Create {
            path: path("src/Only.java"),
            contents: b"x\n".to_vec(),
        })
        .expect("the one permitted operation");
    assert!(
        harness
            .plan(ChangeRequest::Create {
                path: path("src/Another.java"),
                contents: b"y\n".to_vec()
            })
            .is_err(),
        "the budget really is spent"
    );

    harness.undo().expect("undo is still available");
    assert!(!harness.exists("src/Only.java"));
}

#[test]
fn undo_reaches_a_path_the_policy_would_no_longer_allow() {
    // A rule that permits create but not trash would otherwise let the engine
    // make a file it cannot remove.
    let _guard = env_lock();
    let policy = "\
schema_version = 1

[[scope.allow_rule]]
path = \"src/**\"
ops = [\"create\", \"replace\"]
";
    let mut harness = Harness::with_policy(policy);

    harness
        .run(ChangeRequest::Create {
            path: path("src/Stuck.java"),
            contents: b"x\n".to_vec(),
        })
        .expect("create");
    assert!(
        harness
            .plan(ChangeRequest::Trash {
                path: path("src/Stuck.java")
            })
            .is_err(),
        "the policy really does refuse a trash"
    );

    harness.undo().expect("undo is not a new grant");
    assert!(!harness.exists("src/Stuck.java"));
}

#[test]
fn an_undo_is_not_itself_undone() {
    // Without recording which operation a reversal reverses, the second undo
    // picks up the first one and puts the change back — walking in a circle
    // rather than backwards. A test caught exactly that.
    let _guard = env_lock();
    let mut harness = Harness::new();
    harness.write("src/A.java", "one\n");

    harness
        .run(ChangeRequest::Replace {
            path: path("src/A.java"),
            contents: b"two\n".to_vec(),
        })
        .expect("replace");
    harness.undo().expect("undo");
    assert_eq!(harness.read("src/A.java"), "one\n");

    // Nothing is left that undo should reach for.
    let error = harness.undo().unwrap_err();
    assert_eq!(error.code(), ErrorCode::PlanNotFound);
    assert_eq!(harness.read("src/A.java"), "one\n");
}
