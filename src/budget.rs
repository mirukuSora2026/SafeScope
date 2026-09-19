//! How much a task is allowed to change.
//!
//! The limit exists to keep a small edit from sprawling into a refactor nobody
//! asked for. Three rules shape how it is counted, and each one is there because
//! the obvious alternative is wrong.
//!
//! **Undo does not give budget back.** Otherwise `change → undo → change → undo`
//! is an unlimited number of changes, and the limit stops meaning anything. What
//! a task has touched is a fact about the work done, not about what currently
//! remains on disk.
//!
//! **Anything that might have happened counts.** Only an operation the engine
//! has confirmed did not run releases its share, so crashing at the right moment
//! is not a way to spend less than was used.
//!
//! **A changed path is a path that was affected.** A move counts both ends, so
//! "3 of 8 paths" keeps meaning "three paths were affected" — a number a person
//! can check against what they see.

use std::collections::BTreeSet;

use crate::dataformatting::{self, Msg};
use crate::domain::{Phase, Transition};
use crate::error::{Denial, Error, ErrorCode, Result};
use crate::ids::TaskId;
use crate::journal::Journal;
use crate::paths::RelPath;
use crate::policy::BudgetLimits;
use crate::store::content::ContentStore;

/// What a task has spent so far.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BudgetUsage {
    /// Every path the task has affected, whatever became of it since.
    pub changed_paths: BTreeSet<RelPath>,
    pub moves: u64,
    pub operations: u64,
    /// Bytes of recovery data held for the whole workspace, not just this task.
    pub snapshot_bytes: u64,
}

impl BudgetUsage {
    pub fn paths(&self) -> u64 {
        self.changed_paths.len() as u64
    }

    /// Whether any limit is close enough to be worth mentioning.
    pub fn nearing(&self, limits: &BudgetLimits) -> bool {
        let ratio = limits.warn_at_ratio;
        dataformatting::ratio(self.paths(), limits.max_changed_paths) >= ratio
            || dataformatting::ratio(self.operations, limits.max_operations) >= ratio
            || dataformatting::ratio(self.moves, limits.max_moves) >= ratio
            || dataformatting::ratio(self.snapshot_bytes, limits.max_snapshot_bytes) >= ratio
    }
}

/// Reads a task's spending and decides whether one more change fits.
pub struct Budget<'a> {
    pub limits: &'a BudgetLimits,
    pub journal: &'a Journal,
    pub snapshots: &'a ContentStore,
}

impl Budget<'_> {
    /// What the task has spent.
    ///
    /// Derived from the journal rather than kept as a running total, so it
    /// cannot drift out of step with what actually happened.
    pub fn usage(&self, task: TaskId) -> Result<BudgetUsage> {
        let mut usage = BudgetUsage {
            snapshot_bytes: self.snapshots.usage_bytes()?,
            ..BudgetUsage::default()
        };

        for record in self.journal.history(task)? {
            if !record.kind.spends_budget() || !record.stage.holds_budget() {
                continue;
            }
            usage.operations += 1;
            usage.moves += record.transition.budget_cost().moves as u64;
            usage
                .changed_paths
                .extend(record.transition.touched_paths().into_iter().cloned());
        }
        Ok(usage)
    }

    /// Whether one more change fits, without recording anything.
    pub fn check(&self, task: TaskId, transition: &Transition) -> Result<BudgetUsage> {
        let usage = self.usage(task)?;

        if usage.operations >= self.limits.max_operations {
            return Err(exceeded(Msg::BudgetOperationsExceeded {
                used: usage.operations,
                limit: self.limits.max_operations,
            }));
        }

        let moves = transition.budget_cost().moves as u64;
        if usage.moves + moves > self.limits.max_moves {
            return Err(exceeded(Msg::BudgetMovesExceeded {
                used: usage.moves,
                limit: self.limits.max_moves,
            }));
        }

        // The union, not the sum: touching a path twice is one path, which is
        // why the count keeps meaning "paths affected" rather than "edits made".
        let unseen: Vec<&RelPath> = transition
            .touched_paths()
            .into_iter()
            .filter(|path| !usage.changed_paths.contains(*path))
            .collect();
        if usage.paths() + unseen.len() as u64 > self.limits.max_changed_paths {
            return Err(exceeded(Msg::BudgetPathsExceeded {
                used: usage.paths(),
                limit: self.limits.max_changed_paths,
                adding: unseen
                    .first()
                    .map_or_else(String::new, |path| path.as_str().to_owned()),
            }));
        }

        if usage.snapshot_bytes + self.snapshot_need(transition) > self.limits.max_snapshot_bytes {
            return Err(exceeded(Msg::BudgetStorageExceeded {
                used: dataformatting::bytes(usage.snapshot_bytes),
                limit: dataformatting::bytes(self.limits.max_snapshot_bytes),
            }));
        }

        Ok(usage)
    }

    /// Bytes this change would add to the recovery store.
    ///
    /// Counted at full size even when the contents may already be stored.
    /// Over-counting refuses a change that would have fitted; under-counting
    /// lets a change through that leaves nothing to undo it with.
    fn snapshot_need(&self, transition: &Transition) -> u64 {
        if !transition.operation().destroys_content() {
            return 0;
        }
        transition
            .states(Phase::Before)
            .iter()
            .filter_map(|state| match &state.state {
                crate::domain::FileState::Present { len, .. } => Some(*len),
                crate::domain::FileState::Absent => None,
            })
            .sum()
    }
}

fn exceeded(message: Msg) -> Error {
    Error::Denied(
        Denial::new(ErrorCode::BudgetExceeded, message).with_hint(Msg::HintRequestBudgetExpansion),
    )
}
