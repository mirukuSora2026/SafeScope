//! The operation journal.
//!
//! What matters here is the questions the journal can still answer after a
//! crash. A two-state "done or failed" model would force the engine to guess,
//! and guessing is how a change gets applied twice or an intact file gets
//! restored over.

use safescope::domain::{FileState, PathState, Transition};
use safescope::error::ErrorCode;
use safescope::hash::ContentHash;
use safescope::ids::{OperationId, PlanId, RequestId, TaskId};
use safescope::journal::{Journal, NewOperation, Stage};
use safescope::paths::RelPath;
use tempfile::TempDir;

fn path(text: &str) -> RelPath {
    RelPath::parse(text).unwrap()
}

fn hash(text: &str) -> ContentHash {
    ContentHash::of_bytes(text.as_bytes())
}

fn journal() -> (TempDir, Journal) {
    let directory = TempDir::new().expect("temp dir");
    let journal = Journal::open_at(&directory.path().join("state.sqlite")).expect("open");
    (directory, journal)
}

fn replace_transition() -> Transition {
    Transition::replace(path("src/a.rs"), (hash("old"), 3), (hash("new"), 3))
}

#[test]
fn a_prepared_operation_is_readable_immediately() {
    // I1: the record is durable before anything is touched, so everything after
    // it may assume a record exists describing what was intended.
    let (_dir, mut journal) = journal();
    let task = TaskId::new();

    let record = journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()).with_payload(Some(hash("new"))),
            &replace_transition(),
        )
        .expect("record");

    assert_eq!(record.stage, Stage::Prepared);
    let read_back = journal.get(record.id).expect("get");
    assert_eq!(read_back, record);
    assert_eq!(read_back.transition, replace_transition());
}

#[test]
fn the_journal_survives_being_reopened() {
    let directory = TempDir::new().expect("temp dir");
    let database = directory.path().join("state.sqlite");
    let task = TaskId::new();

    let id = {
        let mut journal = Journal::open_at(&database).expect("open");
        journal
            .record_prepared(
                NewOperation::change(task, PlanId::new()),
                &replace_transition(),
            )
            .expect("record")
            .id
    };

    let journal = Journal::open_at(&database).expect("reopen");
    assert_eq!(journal.get(id).expect("get").stage, Stage::Prepared);
}

#[test]
fn sequence_numbers_increase_and_do_not_repeat() {
    // Undo walks backwards by sequence, so a clock that moved backwards must not
    // be able to lose the order.
    let (_dir, mut journal) = journal();
    let task = TaskId::new();

    let mut seen = Vec::new();
    for _ in 0..5 {
        let record = journal
            .record_prepared(
                NewOperation::change(task, PlanId::new()),
                &replace_transition(),
            )
            .expect("record");
        seen.push(record.sequence);
    }

    assert_eq!(seen, [1, 2, 3, 4, 5]);
}

#[test]
fn an_operation_moves_through_its_stages() {
    let (_dir, mut journal) = journal();
    let task = TaskId::new();
    let record = journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()),
            &replace_transition(),
        )
        .expect("record");

    journal
        .mark(record.id, Stage::Applying, None, None)
        .expect("applying");
    assert_eq!(journal.get(record.id).unwrap().stage, Stage::Applying);

    let observed = vec![PathState::present(path("src/a.rs"), hash("new"), 3)];
    journal
        .mark(record.id, Stage::Committed, Some(&observed), None)
        .expect("committed");

    let settled = journal.get(record.id).expect("get");
    assert_eq!(settled.stage, Stage::Committed);
    assert_eq!(settled.observed.as_deref(), Some(&observed[..]));
}

#[test]
fn marking_an_unknown_operation_is_refused() {
    let (_dir, mut journal) = journal();
    let error = journal
        .mark(OperationId::new(), Stage::Committed, None, None)
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::PlanNotFound);
}

#[test]
fn an_unsettled_operation_is_what_recovery_starts_from() {
    let (_dir, mut journal) = journal();
    let task = TaskId::new();

    let crashed = journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()),
            &replace_transition(),
        )
        .expect("record");
    journal
        .mark(crashed.id, Stage::Applying, None, None)
        .expect("applying");

    let finished = journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()),
            &replace_transition(),
        )
        .expect("record");
    journal
        .mark(finished.id, Stage::Committed, None, None)
        .expect("committed");

    let unsettled = journal.unsettled().expect("unsettled");
    let ids: Vec<_> = unsettled.iter().map(|record| record.id).collect();
    assert!(ids.contains(&crashed.id));
    assert!(
        !ids.contains(&finished.id),
        "a settled operation needs nothing"
    );
}

#[test]
fn a_resent_request_returns_the_original_operation() {
    // Otherwise a retry applies the same change twice.
    let (_dir, mut journal) = journal();
    let task = TaskId::new();
    let request = RequestId::new();
    let digest = hash("the request");

    let first = journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()).with_request(request, digest),
            &replace_transition(),
        )
        .expect("record");

    let claimed = journal.claim_request(task, request, digest).expect("claim");
    assert_eq!(claimed.map(|record| record.id), Some(first.id));
}

#[test]
fn the_same_key_with_different_contents_is_refused() {
    // Guessing which was meant could apply a change twice.
    let (_dir, mut journal) = journal();
    let task = TaskId::new();
    let request = RequestId::new();

    journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()).with_request(request, hash("first request")),
            &replace_transition(),
        )
        .expect("record");

    let error = journal
        .claim_request(task, request, hash("different"))
        .unwrap_err();
    assert_eq!(error.code(), ErrorCode::RequestMismatch);
}

#[test]
fn an_unused_request_key_is_free() {
    let (_dir, journal) = journal();
    let claimed = journal
        .claim_request(TaskId::new(), RequestId::new(), hash("new"))
        .expect("claim");
    assert!(claimed.is_none());
}

#[test]
fn a_request_key_belongs_to_one_task() {
    let (_dir, mut journal) = journal();
    let request = RequestId::new();
    let digest = hash("the request");

    journal
        .record_prepared(
            NewOperation::change(TaskId::new(), PlanId::new()).with_request(request, digest),
            &replace_transition(),
        )
        .expect("record");

    let other = journal
        .claim_request(TaskId::new(), request, digest)
        .expect("claim");
    assert!(other.is_none(), "another task's key is not this task's");
}

#[test]
fn history_is_ordered_and_scoped_to_one_task() {
    let (_dir, mut journal) = journal();
    let mine = TaskId::new();
    let theirs = TaskId::new();

    for _ in 0..3 {
        journal
            .record_prepared(
                NewOperation::change(mine, PlanId::new()),
                &replace_transition(),
            )
            .expect("record");
    }
    journal
        .record_prepared(
            NewOperation::change(theirs, PlanId::new()),
            &replace_transition(),
        )
        .expect("record");

    let history = journal.history(mine).expect("history");
    assert_eq!(history.len(), 3);
    assert!(
        history
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
}

#[test]
fn undo_offers_the_most_recent_committed_operation() {
    // Only the last one: reversing an earlier operation would have to account
    // for everything done since.
    let (_dir, mut journal) = journal();
    let task = TaskId::new();

    let first = journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()),
            &replace_transition(),
        )
        .expect("record");
    journal
        .mark(first.id, Stage::Committed, None, None)
        .expect("committed");

    let second = journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()),
            &replace_transition(),
        )
        .expect("record");
    journal
        .mark(second.id, Stage::Committed, None, None)
        .expect("committed");

    let pending = journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()),
            &replace_transition(),
        )
        .expect("record");
    journal
        .mark(pending.id, Stage::Applying, None, None)
        .expect("applying");

    let candidate = journal.last_undoable(task).expect("last").expect("one");
    assert_eq!(
        candidate.id, second.id,
        "not the unfinished one, not the older one"
    );
    assert!(candidate.is_undoable());
}

#[test]
fn nothing_is_undoable_before_anything_commits() {
    let (_dir, mut journal) = journal();
    let task = TaskId::new();

    let record = journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()),
            &replace_transition(),
        )
        .expect("record");
    journal
        .mark(record.id, Stage::Aborted, None, None)
        .expect("aborted");

    assert!(journal.last_undoable(task).expect("last").is_none());
}

#[test]
fn every_transition_shape_round_trips() {
    let (_dir, mut journal) = journal();
    let task = TaskId::new();

    let transitions = [
        Transition::create(path("a.rs"), hash("v1"), 2),
        Transition::replace(path("b.rs"), (hash("old"), 3), (hash("new"), 3)),
        Transition::trash(path("c.rs"), hash("gone"), 4),
        Transition::rename(path("d.rs"), path("e.rs"), hash("same"), 4),
    ];

    for transition in &transitions {
        let record = journal
            .record_prepared(NewOperation::change(task, PlanId::new()), transition)
            .expect("record");
        assert_eq!(&journal.get(record.id).expect("get").transition, transition);
    }
}

#[test]
fn an_observed_absence_round_trips() {
    let (_dir, mut journal) = journal();
    let task = TaskId::new();
    let record = journal
        .record_prepared(
            NewOperation::change(task, PlanId::new()),
            &Transition::trash(path("gone.rs"), hash("content"), 7),
        )
        .expect("record");

    let observed = vec![PathState::absent(path("gone.rs"))];
    journal
        .mark(record.id, Stage::Committed, Some(&observed), None)
        .expect("commit");

    let settled = journal.get(record.id).expect("get");
    assert_eq!(
        settled.observed.expect("observed")[0].state,
        FileState::Absent
    );
}
