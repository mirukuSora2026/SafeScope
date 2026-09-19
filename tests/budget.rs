//! How much a task is allowed to change.
//!
//! The numbers matter less than what they count. A limit that counted edits
//! rather than paths, or that gave budget back when work was undone, would stop
//! meaning what a person reads it to mean.

mod harness;

use harness::{Harness, env_lock, path};
use safescope::budget::Budget;
use safescope::domain::Transition;
use safescope::error::ErrorCode;
use safescope::hash::ContentHash;
use safescope::ids::PlanId;
use safescope::journal::{OperationKind, Stage};
use safescope::planner::ChangeRequest;

/// Room for three paths, four operations and one move.
const TIGHT: &str = "\
schema_version = 1

[scope]
allow = [\"src/**\"]

[budget]
max_changed_paths = 3
max_operations = 4
max_moves = 1
";

fn budget<'a>(harness: &'a Harness) -> Budget<'a> {
    Budget {
        limits: harness.policy.budget(),
        journal: &harness.journal,
        snapshots: &harness.snapshots,
    }
}

#[test]
fn a_fresh_task_has_spent_nothing() {
    let _guard = env_lock();
    let harness = Harness::with_policy(TIGHT);
    let usage = budget(&harness).usage(harness.task).expect("usage");

    assert_eq!(usage.paths(), 0);
    assert_eq!(usage.operations, 0);
    assert_eq!(usage.moves, 0);
}

#[test]
fn editing_one_file_twice_spends_one_path_and_two_operations() {
    // The count means "paths affected", not "edits made" — which is what lets a
    // person check it against what they can see.
    let _guard = env_lock();
    let mut harness = Harness::with_policy(TIGHT);

    harness
        .run(ChangeRequest::Create {
            path: path("src/A.java"),
            contents: b"one\n".to_vec(),
        })
        .expect("create");
    harness
        .run(ChangeRequest::Replace {
            path: path("src/A.java"),
            contents: b"two\n".to_vec(),
        })
        .expect("replace");

    let usage = budget(&harness).usage(harness.task).expect("usage");
    assert_eq!(usage.paths(), 1);
    assert_eq!(usage.operations, 2);
}

#[test]
fn a_move_counts_both_ends() {
    let _guard = env_lock();
    let mut harness = Harness::with_policy(TIGHT);
    harness.write("src/A.java", "content\n");

    harness
        .run(ChangeRequest::Move {
            from: path("src/A.java"),
            to: path("src/B.java"),
        })
        .expect("move");

    let usage = budget(&harness).usage(harness.task).expect("usage");
    assert_eq!(
        usage.paths(),
        2,
        "the file left one path and arrived at another"
    );
    assert_eq!(usage.moves, 1);
}

#[test]
fn a_fourth_path_is_refused_and_nothing_is_written() {
    let _guard = env_lock();
    let mut harness = Harness::with_policy(TIGHT);

    for name in ["A", "B", "C"] {
        harness
            .run(ChangeRequest::Create {
                path: path(&format!("src/{name}.java")),
                contents: b"x\n".to_vec(),
            })
            .expect("create");
    }

    let error = harness
        .plan(ChangeRequest::Create {
            path: path("src/D.java"),
            contents: b"x\n".to_vec(),
        })
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::BudgetExceeded);
    assert!(!harness.exists("src/D.java"));
    assert!(
        error.report().hint.is_some(),
        "the caller is told what to do instead"
    );
}

#[test]
fn a_further_edit_to_an_already_counted_path_still_fits() {
    // The path limit is a union, so work can continue on what has been touched
    // even once no new file may be opened.
    let _guard = env_lock();
    let mut harness = Harness::with_policy(TIGHT);

    for name in ["A", "B", "C"] {
        harness
            .run(ChangeRequest::Create {
                path: path(&format!("src/{name}.java")),
                contents: b"x\n".to_vec(),
            })
            .expect("create");
    }

    harness
        .run(ChangeRequest::Replace {
            path: path("src/A.java"),
            contents: b"y\n".to_vec(),
        })
        .expect("a path already counted stays available");
    assert_eq!(harness.read("src/A.java"), "y\n");
}

#[test]
fn running_out_of_operations_is_refused() {
    let _guard = env_lock();
    let mut harness = Harness::with_policy(TIGHT);

    for name in ["A", "B", "C"] {
        harness
            .run(ChangeRequest::Create {
                path: path(&format!("src/{name}.java")),
                contents: b"x\n".to_vec(),
            })
            .expect("create");
    }
    harness
        .run(ChangeRequest::Replace {
            path: path("src/A.java"),
            contents: b"y\n".to_vec(),
        })
        .expect("the fourth operation");

    let error = harness
        .plan(ChangeRequest::Replace {
            path: path("src/B.java"),
            contents: b"z\n".to_vec(),
        })
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::BudgetExceeded);
    assert_eq!(harness.read("src/B.java"), "x\n");
}

#[test]
fn a_second_move_is_refused() {
    let _guard = env_lock();
    let mut harness = Harness::with_policy(TIGHT);
    harness.write("src/A.java", "content\n");

    harness
        .run(ChangeRequest::Move {
            from: path("src/A.java"),
            to: path("src/B.java"),
        })
        .expect("first move");

    let error = harness
        .plan(ChangeRequest::Move {
            from: path("src/B.java"),
            to: path("src/C.java"),
        })
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::BudgetExceeded);
    assert_eq!(harness.read("src/B.java"), "content\n");
}

#[test]
fn an_operation_confirmed_not_to_have_run_gives_its_share_back() {
    let _guard = env_lock();
    let mut harness = Harness::with_policy(TIGHT);
    let transition = Transition::create(path("src/Ghost.java"), ContentHash::of_bytes(b"x"), 1);

    let record = harness
        .journal
        .record_prepared(
            harness.task,
            PlanId::new(),
            OperationKind::Change,
            None,
            None,
            &transition,
            None,
        )
        .expect("record");

    // While it might have happened, it is held against the budget.
    assert_eq!(budget(&harness).usage(harness.task).unwrap().paths(), 1);

    harness
        .journal
        .mark(record.id, Stage::Aborted, None, None)
        .expect("abort");
    assert_eq!(
        budget(&harness).usage(harness.task).unwrap().paths(),
        0,
        "a confirmed non-event costs nothing"
    );
}

#[test]
fn an_unfinished_operation_keeps_holding_its_share() {
    // Crashing at the right moment must not be a way to spend less than was used.
    let _guard = env_lock();
    let mut harness = Harness::with_policy(TIGHT);
    let transition = Transition::create(path("src/Maybe.java"), ContentHash::of_bytes(b"x"), 1);

    let record = harness
        .journal
        .record_prepared(
            harness.task,
            PlanId::new(),
            OperationKind::Change,
            None,
            None,
            &transition,
            None,
        )
        .expect("record");
    harness
        .journal
        .mark(record.id, Stage::Applying, None, None)
        .expect("applying");

    assert_eq!(budget(&harness).usage(harness.task).unwrap().operations, 1);
}

#[test]
fn undo_does_not_spend_the_change_budget() {
    let _guard = env_lock();
    let mut harness = Harness::with_policy(TIGHT);
    let transition = Transition::create(path("src/Undone.java"), ContentHash::of_bytes(b"x"), 1);

    let record = harness
        .journal
        .record_prepared(
            harness.task,
            PlanId::new(),
            OperationKind::Undo,
            None,
            None,
            &transition,
            None,
        )
        .expect("record");
    harness
        .journal
        .mark(record.id, Stage::Committed, None, None)
        .expect("commit");

    let usage = budget(&harness).usage(harness.task).expect("usage");
    assert_eq!(usage.operations, 0);
    assert_eq!(usage.paths(), 0);
}

#[test]
fn budget_belongs_to_one_task() {
    let _guard = env_lock();
    let mut harness = Harness::with_policy(TIGHT);
    let other = safescope::ids::TaskId::new();

    for name in ["A", "B", "C"] {
        harness
            .journal
            .record_prepared(
                other,
                PlanId::new(),
                OperationKind::Change,
                None,
                None,
                &Transition::create(
                    path(&format!("src/{name}.java")),
                    ContentHash::of_bytes(b"x"),
                    1,
                ),
                None,
            )
            .expect("record");
    }

    assert_eq!(budget(&harness).usage(harness.task).unwrap().paths(), 0);
    assert_eq!(budget(&harness).usage(other).unwrap().paths(), 3);
}

#[test]
fn storage_is_measured_across_the_whole_workspace() {
    // Snapshots are shared, so the limit is a property of the store rather than
    // of one task.
    let _guard = env_lock();
    let mut harness = Harness::with_policy(TIGHT);
    harness.write("src/A.java", "original contents\n");

    harness
        .run(ChangeRequest::Replace {
            path: path("src/A.java"),
            contents: b"new\n".to_vec(),
        })
        .expect("replace");

    let usage = budget(&harness).usage(harness.task).expect("usage");
    assert_eq!(usage.snapshot_bytes, 18, "the previous contents are held");
}

#[test]
fn a_change_too_large_for_the_recovery_store_is_refused() {
    let _guard = env_lock();
    let policy = "\
schema_version = 1

[scope]
allow = [\"src/**\"]

[budget]
max_file_bytes = 4096
max_snapshot_bytes = 8
";
    let harness = Harness::with_policy(policy);
    harness.write("src/A.java", "far more than eight bytes\n");

    let error = harness
        .plan(ChangeRequest::Replace {
            path: path("src/A.java"),
            contents: b"new\n".to_vec(),
        })
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::BudgetExceeded);
    assert_eq!(
        harness.read("src/A.java"),
        "far more than eight bytes\n",
        "a change that could not be undone is not made"
    );
}
