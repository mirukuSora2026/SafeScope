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

/// What the journal is told when the filesystem work returned an error.
///
/// Every operation here mutates and then makes the mutation durable, so a
/// failure can arrive after the change has already landed — a rename that
/// succeeded whose fsync then failed. The executor recorded `Aborted` for all of
/// them, which released budget for work that ran, left it with no undo, and made
/// drift report the engine's own change as somebody else's.
mod after_a_failed_operation {
    use safescope::domain::Observation;
    use safescope::executor::stage_for;
    use safescope::journal::Stage;

    #[test]
    fn nothing_changed_is_aborted() {
        assert_eq!(stage_for(Observation::MatchesBefore), Stage::Aborted);
    }

    #[test]
    fn the_change_landed_is_not_aborted() {
        // The case the old code got wrong. Anything but `Aborted` will do here —
        // what matters is that the record does not claim nothing happened.
        let stage = stage_for(Observation::MatchesAfter);
        assert_ne!(
            stage,
            Stage::Aborted,
            "a change that ran was called a no-op"
        );
        assert_eq!(stage, Stage::Committed);
    }

    #[test]
    fn a_half_finished_change_is_left_for_a_person() {
        assert_eq!(stage_for(Observation::Partial), Stage::RecoveryRequired);
        assert!(Stage::RecoveryRequired.needs_attention());
    }

    #[test]
    fn a_workspace_matching_neither_side_is_a_conflict() {
        assert_eq!(stage_for(Observation::Divergent), Stage::Conflict);
        assert!(Stage::Conflict.needs_attention());
    }

    #[test]
    fn the_stages_that_mean_it_ran_carry_an_observation() {
        // Drift decides what the engine did from the observations on committed
        // records. A record settled without one says the workspace moved and
        // offers no evidence of it, so drift reports the engine's own change as
        // somebody else's — and recovery, asking the same question after a
        // crash, records it. The two must not disagree.
        for stage in [Stage::Committed, Stage::Conflict] {
            assert!(
                safescope::executor::settles_with_an_observation(stage),
                "{stage:?} would be recorded with nothing to show for it"
            );
        }
        for stage in [Stage::Aborted, Stage::RecoveryRequired, Stage::Rejected] {
            assert!(
                !safescope::executor::settles_with_an_observation(stage),
                "{stage:?} claimed an observation it has no business making"
            );
        }
    }

    #[test]
    fn only_aborted_gives_the_budget_back() {
        // I5: released only if nothing ran. The three stages that mean it may
        // have run must all keep holding it.
        assert!(!Stage::Aborted.holds_budget());
        for stage in [Stage::Committed, Stage::RecoveryRequired, Stage::Conflict] {
            assert!(
                stage.holds_budget(),
                "{stage:?} released budget for work that may have run"
            );
        }
    }
}
