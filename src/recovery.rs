//! Working out what happened when the engine stopped mid-operation.
//!
//! Recovery is the reason the journal and the plan are shaped the way they are.
//! Because a plan records the exact state expected on both sides, and because
//! the applying record is durable before the first filesystem call, a crash
//! leaves a decidable question rather than a guess:
//!
//! | Stage when it stopped | What is on disk      | Conclusion         |
//! |-----------------------|----------------------|--------------------|
//! | `Prepared`            | anything             | it never started   |
//! | `Applying`            | the before-state     | it did not run     |
//! | `Applying`            | the after-state      | it ran             |
//! | `Applying`            | part of each         | compare by hand    |
//! | `Applying`            | neither              | conflict           |
//!
//! Nothing here repairs anything. A conclusion that the engine cannot reach is
//! reported for a person to look at, because the situations it cannot decide are
//! exactly the ones where being wrong destroys work.

use crate::domain::PathState;
use crate::error::Result;
use crate::ids::OperationId;
use crate::journal::{Journal, OperationRecord, Stage};
use crate::path_guard::Workspace;
use crate::paths::TMP_PREFIX;

/// What recovery concluded about one operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conclusion {
    pub operation: OperationId,
    pub was: Stage,
    pub now: Stage,
}

impl Conclusion {
    pub const fn needs_attention(&self) -> bool {
        self.now.needs_attention()
    }
}

/// Everything one pass of recovery found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    pub conclusions: Vec<Conclusion>,
    /// Temporaries a crash left behind, now removed.
    pub temporaries_removed: Vec<String>,
}

impl RecoveryReport {
    pub fn is_empty(&self) -> bool {
        self.conclusions.is_empty() && self.temporaries_removed.is_empty()
    }

    /// Operations a person has to compare before the workspace is trustworthy.
    pub fn unresolved(&self) -> impl Iterator<Item = &Conclusion> {
        self.conclusions
            .iter()
            .filter(|entry| entry.needs_attention())
    }

    pub fn settled(&self, stage: Stage) -> usize {
        self.conclusions
            .iter()
            .filter(|entry| entry.now == stage)
            .count()
    }
}

/// Reconciles the journal with the workspace.
pub struct Recovery<'a> {
    pub workspace: &'a Workspace,
    pub journal: &'a mut Journal,
}

impl Recovery<'_> {
    /// Examines every unsettled operation and records what can be concluded.
    pub fn run(&mut self) -> Result<RecoveryReport> {
        let mut report = RecoveryReport::default();

        for record in self.journal.unsettled()? {
            let conclusion = self.conclude(&record)?;
            if conclusion.now != conclusion.was {
                self.journal.mark(
                    record.id,
                    conclusion.now,
                    self.observed_for(&record, conclusion.now)?.as_deref(),
                    error_code(conclusion.now),
                )?;
            }
            report.conclusions.push(conclusion);
            report.temporaries_removed.extend(self.sweep(&record)?);
        }

        Ok(report)
    }

    /// The stage this operation should now be in.
    fn conclude(&self, record: &OperationRecord) -> Result<Conclusion> {
        let now = match record.stage {
            // The applying record is committed before the first filesystem call,
            // so an operation still at Prepared cannot have touched anything.
            // This holds whatever is on disk: if the contents have changed, that
            // was somebody else.
            Stage::Prepared => Stage::Aborted,

            // Through the same rule the executor uses when its own filesystem
            // work failed. Two copies of this table would be two chances for it
            // to be answered differently.
            Stage::Applying | Stage::RecoveryRequired => {
                crate::executor::stage_for(record.transition.classify(&self.observe(record)?))
            }

            settled => settled,
        };

        Ok(Conclusion {
            operation: record.id,
            was: record.stage,
            now,
        })
    }

    /// Reads the current state of every path the operation touches.
    fn observe(&self, record: &OperationRecord) -> Result<Vec<PathState>> {
        record
            .transition
            .touched_paths()
            .into_iter()
            .map(|path| {
                self.workspace
                    .resolve(path)
                    .map(|resolved| PathState::new(path.clone(), resolved.state().clone()))
            })
            .collect()
    }

    /// The observation worth recording, which is only the one that settles it.
    fn observed_for(&self, record: &OperationRecord, now: Stage) -> Result<Option<Vec<PathState>>> {
        match now {
            Stage::Committed | Stage::Conflict => self.observe(record).map(Some),
            _ => Ok(None),
        }
    }

    /// Removes temporaries a crash left beside this operation's targets.
    ///
    /// Scoped to the directories the operation was working in rather than the
    /// whole tree: a sweep of the workspace would be slow and would touch files
    /// that have nothing to do with the engine.
    fn sweep(&self, record: &OperationRecord) -> Result<Vec<String>> {
        let mut removed = Vec::new();

        for path in record.transition.touched_paths() {
            let Ok(resolved) = self.workspace.resolve(path) else {
                continue;
            };
            let Ok(entries) = resolved.parent().entries() else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with(TMP_PREFIX) && resolved.parent().remove_file(&name).is_ok() {
                    removed.push(name);
                }
            }
        }

        removed.sort_unstable();
        removed.dedup();
        Ok(removed)
    }
}

/// The code recorded against an operation recovery could not settle cleanly.
fn error_code(stage: Stage) -> Option<&'static str> {
    match stage {
        Stage::Conflict => Some(crate::error::ErrorCode::RecoveryConflict.as_str()),
        Stage::RecoveryRequired => Some(crate::error::ErrorCode::StateUncertain.as_str()),
        _ => None,
    }
}
