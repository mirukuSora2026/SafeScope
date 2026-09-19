//! Reversing the last thing the engine did.
//!
//! Undo goes back one operation at a time, most recent first. Reversing an
//! earlier one would have to account for everything done since — a replace
//! undone under a later move has to know where the file went — and that analysis
//! is a separate feature, not a special case of this one.
//!
//! Two decisions shape it.
//!
//! **Undo is not policy-checked.** It reverses what the engine did under an
//! approved policy, which is not a new grant. Checking it would trap people: a
//! rule allowing `create` and `replace` but not `trash` would let the engine
//! create a file and then refuse to remove it. Protected paths still apply.
//!
//! **Undo never overwrites a later change.** If the file has moved on since the
//! engine touched it, the conflict is reported and nothing is done. The recovery
//! data stays where it is, so a person can compare and decide.

use crate::dataformatting::Msg;
use crate::domain::{Observation, Operation, PathState, Phase};
use crate::error::{Denial, Error, ErrorCode, Result};
use crate::ids::{OperationId, TaskId};
use crate::journal::{Journal, OperationRecord};
use crate::path_guard::Workspace;
use crate::planner::{ChangePlan, ChangeRequest, Planner};
use crate::policy::{Authority, CompiledPolicy, EvaluationContext, PolicyVersion};
use crate::store::content::ContentStore;

/// A plan to reverse a particular operation.
#[derive(Debug, Clone)]
pub struct UndoPlan {
    /// The operation being reversed.
    pub reverses: OperationId,
    pub plan: ChangePlan,
}

/// Builds reversals.
///
/// Only reads, so it borrows the journal shared. Applying the result goes
/// through the ordinary executor: a reversal is a file change and deserves the
/// same steps, the same records and the same crash behaviour as any other.
pub struct Undo<'a> {
    pub workspace: &'a Workspace,
    pub policy: &'a CompiledPolicy,
    pub journal: &'a Journal,
    pub snapshots: &'a ContentStore,
    pub staging: &'a ContentStore,
}

impl Undo<'_> {
    /// Prepares a reversal of the most recent completed operation.
    ///
    /// Changes nothing in the workspace; the caller can show it to a person
    /// first.
    pub fn prepare(&self, task: TaskId, policy_version: PolicyVersion) -> Result<UndoPlan> {
        let record = self
            .journal
            .last_undoable(task)?
            .ok_or_else(nothing_to_undo)?;

        // The file must still be as the engine left it. Anything else and undo
        // would overwrite work that arrived afterwards.
        self.require_untouched(&record)?;

        let request = self.reverse_of(&record)?;
        let context = EvaluationContext {
            grants: &[],
            task,
            policy_version,
            now: std::time::SystemTime::now(),
            authority: Authority::Reversal,
        };

        let plan = Planner {
            workspace: self.workspace,
            policy: self.policy,
            journal: self.journal,
            snapshots: self.snapshots,
            staging: self.staging,
        }
        .plan(&request, &context)?;

        Ok(UndoPlan {
            reverses: record.id,
            plan,
        })
    }

    /// Confirms the workspace still holds what the operation left behind.
    fn require_untouched(&self, record: &OperationRecord) -> Result<()> {
        let observed = self.observe(record)?;
        if record.transition.classify(&observed) == Observation::MatchesAfter {
            return Ok(());
        }

        let culprit = observed
            .iter()
            .find(|seen| record.transition.expected(Phase::After, &seen.path) != Some(&seen.state))
            .map_or_else(
                || record.transition.touched_paths()[0].as_str().to_owned(),
                |seen| seen.path.as_str().to_owned(),
            );

        Err(Error::Denied(
            Denial::new(
                ErrorCode::RecoveryConflict,
                Msg::UndoConflictAt {
                    path: culprit.clone(),
                },
            )
            .with_hint(Msg::HintCompareBeforeUndoing { path: culprit }),
        ))
    }

    /// The request that undoes this operation.
    ///
    /// Expressed as an ordinary request so it goes through the same planner as
    /// everything else — the same state checks, the same snapshot, the same
    /// staging. A separate path through those guarantees is a separate place for
    /// them to be wrong.
    fn reverse_of(&self, record: &OperationRecord) -> Result<ChangeRequest> {
        let before = record.transition.states(Phase::Before);
        let after = record.transition.states(Phase::After);

        Ok(match record.transition.operation() {
            Operation::Create => ChangeRequest::Trash {
                path: after[0].path.clone(),
            },

            Operation::Replace => ChangeRequest::Replace {
                path: before[0].path.clone(),
                contents: self.stored_contents(&before[0])?,
            },

            Operation::Trash => ChangeRequest::Create {
                path: before[0].path.clone(),
                contents: self.stored_contents(&before[0])?,
            },

            Operation::Move => {
                let from = Self::existing(after);
                let to = Self::existing(before);
                ChangeRequest::Move { from, to }
            }
        })
    }

    /// Reads what a path held before the operation, from the snapshot store.
    fn stored_contents(&self, state: &PathState) -> Result<Vec<u8>> {
        let hash = state
            .state
            .hash()
            .expect("a destructive operation records the contents it replaced");
        self.snapshots.read(*hash)
    }

    /// The path that holds the file on one side of a move.
    fn existing(states: &[PathState]) -> crate::paths::RelPath {
        states
            .iter()
            .find(|state| state.state.exists())
            .expect("one side of a move always holds the file")
            .path
            .clone()
    }

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
}

fn nothing_to_undo() -> Error {
    Error::Denied(Denial::new(
        ErrorCode::PlanNotFound,
        Msg::UndoNothingRecorded,
    ))
}
