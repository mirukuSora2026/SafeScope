//! How long recovery data is kept, and what removing it costs.
//!
//! A snapshot is what makes an operation undoable. Keeping every one forever is
//! not free and not harmless: the snapshot store counts against the budget, and
//! `max_snapshot_bytes` is a workspace-wide ceiling. With nothing ever removed,
//! a workspace that has done enough work eventually refuses *every* change, in
//! every task, and the only way out is to delete the store by hand — which
//! throws away all of it rather than the part nobody needs.
//!
//! So `retain_closed_task_days` is what it always said it was: after that, the
//! snapshots of a finished task go, and the operations they belonged to stop
//! being undoable. The journal keeps the record of what happened either way —
//! history is not what is being reclaimed here.
//!
//! # What is never removed
//!
//! - Anything the current task might still undo.
//! - Anything belonging to an operation the engine did not finish, when
//!   `protect_incomplete_tasks` is set. Recovery has not yet said what became of
//!   it, and a snapshot is the only thing that can answer that.
//! - Anything still referenced by a record that is being kept. Blobs are content
//!   addressed and therefore shared, so this is a sweep over what survives, not
//!   a delete alongside each record.
//!
//! Removal fails closed: a record whose age cannot be established is kept.

use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use crate::domain::Phase;
use crate::error::Result;
use crate::hash::ContentHash;
use crate::ids::TaskId;
use crate::journal::{Journal, OperationRecord};
use crate::policy::RecoverySettings;
use crate::store::content::ContentStore;

/// What a sweep did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reclaimed {
    pub blobs: usize,
    pub bytes: u64,
}

impl Reclaimed {
    pub const fn is_empty(&self) -> bool {
        self.blobs == 0
    }
}

/// Removes snapshots no retained operation still needs.
///
/// `now` is passed rather than read so a test can age a workspace without
/// waiting a fortnight for it.
pub fn sweep(
    journal: &Journal,
    snapshots: &ContentStore,
    settings: &RecoverySettings,
    current: TaskId,
    now: SystemTime,
) -> Result<Reclaimed> {
    let window = Duration::from_secs(u64::from(settings.retain_closed_task_days) * 24 * 60 * 60);

    let mut keep: HashSet<ContentHash> = HashSet::new();
    for record in journal.all()? {
        if is_retained(&record, settings, current, now, window) {
            keep.extend(referenced_snapshots(&record));
        }
    }
    snapshots.retain(&keep)
}

/// Whether this record's recovery data is still owed to somebody.
fn is_retained(
    record: &OperationRecord,
    settings: &RecoverySettings,
    current: TaskId,
    now: SystemTime,
    window: Duration,
) -> bool {
    // The task in progress is not a closed task, whatever its records' ages.
    if record.task == current {
        return true;
    }
    if settings.protect_incomplete_tasks && !record.stage.is_settled() {
        return true;
    }
    // A clock that went backwards, or a record from the future: kept. An age
    // that cannot be established is not an age past the window.
    let Ok(age) = now.duration_since(record.updated_at) else {
        return true;
    };
    age < window
}

/// The snapshot hashes an operation would need in order to be undone.
///
/// The before state, which is the only thing that can put the file back.
fn referenced_snapshots(record: &OperationRecord) -> Vec<ContentHash> {
    record
        .transition
        .states(Phase::Before)
        .iter()
        .filter_map(|state| state.state.hash().copied())
        .collect()
}
