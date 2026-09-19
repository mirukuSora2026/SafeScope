//! Carrying out a plan.
//!
//! The order of the steps is the design, and each one exists because of an
//! invariant:
//!
//! ```text
//!  1. the plan has not expired
//!  2. a resent request returns the original result           (no double apply)
//!  3. the workspace still matches the plan's before-state    I3
//!  4. the intent record is committed                         I1
//!  5. recovery data is present and verifies                  I2
//!  6. the applying record is committed
//!  7. the file changes, through the handle resolution gave   I4
//!  8. the result is observed and recorded
//! ```
//!
//! Steps 4 and 6 are separate on purpose. A crash between them leaves a record
//! saying an operation was prepared but never started; a crash after 6 leaves one
//! saying it was under way. Recovery treats those differently, and collapsing
//! them would make the second indistinguishable from the first.
//!
//! When a step fails, the operation is settled as `Aborted` rather than left
//! open — but only where the engine can say nothing happened. A rename either
//! took place or did not, so a rename that returned an error changed nothing.

use std::time::SystemTime;

use crate::budget::Budget;
use crate::domain::{Operation, PathState, Phase};
use crate::error::{Error, Result};
use crate::fault::{self, FaultPoint};
use crate::hash::ContentHash;
use crate::ids::{OperationId, RequestId};
use crate::journal::{Journal, NewOperation, OperationKind, OperationRecord, Stage};
use crate::path_guard::Workspace;
use crate::paths::RelPath;
use crate::planner::ChangePlan;
use crate::platform;
use crate::policy::BudgetLimits;
use crate::store::content::ContentStore;

/// Applies plans to a workspace.
pub struct Executor<'a> {
    pub workspace: &'a Workspace,
    pub journal: &'a mut Journal,
    pub limits: &'a BudgetLimits,
    pub snapshots: &'a ContentStore,
    pub staging: &'a ContentStore,
}

/// The caller's idempotency key and the request it stands for.
#[derive(Debug, Clone, Copy)]
pub struct RequestKey {
    pub id: RequestId,
    /// Hash of the request, so a resend can be told from a different request
    /// that happened to reuse the key.
    pub digest: ContentHash,
}

impl Executor<'_> {
    /// Applies a plan, returning the journal record of what happened.
    pub fn apply(
        &mut self,
        plan: &ChangePlan,
        request: Option<RequestKey>,
    ) -> Result<OperationRecord> {
        self.apply_as(plan, OperationKind::Change, request)
    }

    /// Applies a plan, recording what it is for.
    ///
    /// Undo goes through the same steps — it is a file change like any other and
    /// deserves the same record — but is marked so it does not spend the change
    /// budget.
    pub fn apply_as(
        &mut self,
        plan: &ChangePlan,
        kind: OperationKind,
        request: Option<RequestKey>,
    ) -> Result<OperationRecord> {
        self.apply_reversing(plan, kind, None, request)
    }

    /// Applies a plan that reverses `reverses`.
    ///
    /// Recording which operation was reversed is what stops a second undo from
    /// reversing the first one and walking in a circle.
    pub fn apply_reversing(
        &mut self,
        plan: &ChangePlan,
        kind: OperationKind,
        reverses: Option<OperationId>,
        request: Option<RequestKey>,
    ) -> Result<OperationRecord> {
        if plan.has_expired(SystemTime::now()) {
            return Err(Error::Denied(plan.expiry_denial()));
        }

        // A resend returns what the first attempt produced. Acting again would
        // apply the same change twice.
        if let Some(key) = request
            && let Some(existing) = self.journal.claim_request(plan.task, key.id, key.digest)?
        {
            return Ok(existing);
        }

        // I3: the plan describes a particular state of the workspace, and this
        // is the last moment at which that can be confirmed.
        self.require_unchanged(plan)?;

        // I5: checked immediately before the reservation, in the same
        // single-writer window, so nothing can be spent in between. Undo is
        // exempt — see OperationKind.
        if kind.spends_budget() {
            Budget {
                limits: self.limits,
                journal: self.journal,
                snapshots: self.snapshots,
            }
            .check(plan.task, &plan.transition)?;
        }

        // I1: durable before anything is touched.
        let mut new = NewOperation {
            task: plan.task,
            plan: plan.id,
            kind,
            reverses,
            request: None,
            request_digest: None,
            payload: plan.payload,
        };
        if let Some(key) = request {
            new = new.with_request(key.id, key.digest);
        }

        let record = self.journal.record_prepared(new, &plan.transition)?;
        fault::check(FaultPoint::AfterPreparedRecord);

        // I2: for a destructive operation the snapshot was stored at planning
        // time; this confirms it is still there and still reads back correctly
        // before anything is put at risk.
        if let Err(error) = self.require_recoverable(plan) {
            self.settle(&record, Stage::Rejected, &error);
            return Err(error);
        }

        self.journal.mark(record.id, Stage::Applying, None, None)?;
        fault::check(FaultPoint::AfterApplyingRecord);

        if let Err(error) = self.perform(plan) {
            // Every filesystem step either happened or did not, so a failure
            // here means nothing changed.
            self.settle(&record, Stage::Aborted, &error);
            return Err(error);
        }
        fault::check(FaultPoint::AfterRenameBeforeCommit);

        let observed = self.observe(plan.transition.touched_paths())?;
        self.journal
            .mark(record.id, Stage::Committed, Some(&observed), None)?;
        self.journal.get(record.id)
    }

    /// Confirms the workspace still holds what the plan was built against.
    fn require_unchanged(&self, plan: &ChangePlan) -> Result<()> {
        let observed = self.observe(plan.transition.touched_paths())?;
        if plan.transition.matches(Phase::Before, &observed) {
            return Ok(());
        }
        let culprit = observed
            .iter()
            .find(|seen| plan.transition.expected(Phase::Before, &seen.path) != Some(&seen.state))
            .map_or_else(
                || plan.transition.touched_paths()[0].clone(),
                |seen| seen.path.clone(),
            );
        Err(crate::planner::state_changed(&culprit))
    }

    /// Confirms recovery data exists for what the operation will destroy.
    fn require_recoverable(&self, plan: &ChangePlan) -> Result<()> {
        if !plan.transition.operation().destroys_content() {
            return Ok(());
        }
        for state in plan.transition.states(Phase::Before) {
            if let Some(hash) = state.state.hash() {
                self.snapshots.read(*hash)?;
            }
        }
        Ok(())
    }

    /// Does the filesystem work, through the handles resolution produced.
    fn perform(&self, plan: &ChangePlan) -> Result<()> {
        match plan.transition.operation() {
            Operation::Create => {
                let (resolved, contents) = self.prepare_write(plan)?;
                let staged = platform::stage(resolved.parent(), &contents)?;
                fault::check(FaultPoint::AfterTemporaryWriteBeforeRename);
                staged.promote_new(resolved.file_name())
            }

            Operation::Replace => {
                let (resolved, contents) = self.prepare_write(plan)?;
                let staged = platform::stage(resolved.parent(), &contents)?;
                fault::check(FaultPoint::AfterTemporaryWriteBeforeRename);
                staged.promote_over(resolved.file_name())
            }

            Operation::Trash => {
                let path = &plan.transition.states(Phase::Before)[0].path;
                let resolved = self.workspace.resolve(path)?;
                fault::check(FaultPoint::AfterSnapshotBeforeUnlink);
                platform::remove(resolved.parent(), resolved.file_name())
            }

            Operation::Move => {
                let (from, to) = self.move_paths(plan);
                let source = self.workspace.resolve(from)?;
                let destination = self.workspace.resolve(to)?;
                platform::move_file(
                    source.parent(),
                    source.file_name(),
                    destination.parent(),
                    destination.file_name(),
                )
            }
        }
    }

    /// The target and the staged contents for an operation that writes.
    fn prepare_write(&self, plan: &ChangePlan) -> Result<(crate::path_guard::Resolved, Vec<u8>)> {
        let path = &plan.transition.states(Phase::After)[0].path;
        let hash = plan
            .payload
            .expect("a writing operation always has a payload");
        // Read back through the store, which verifies the hash: the bytes about
        // to be written are the bytes the plan promised.
        let contents = self.staging.read(hash)?;
        Ok((self.workspace.resolve(path)?, contents))
    }

    fn move_paths<'p>(&self, plan: &'p ChangePlan) -> (&'p RelPath, &'p RelPath) {
        let before = plan.transition.states(Phase::Before);
        let from = &before
            .iter()
            .find(|state| state.state.exists())
            .expect("a move starts from a file that exists")
            .path;
        let to = &plan
            .transition
            .states(Phase::After)
            .iter()
            .find(|state| state.state.exists())
            .expect("a move ends at a file that exists")
            .path;
        (from, to)
    }

    /// Reads the current state of every path a transition touches.
    fn observe(&self, paths: Vec<&RelPath>) -> Result<Vec<PathState>> {
        paths
            .into_iter()
            .map(|path| {
                self.workspace
                    .resolve(path)
                    .map(|resolved| PathState::new(path.clone(), resolved.state().clone()))
            })
            .collect()
    }

    /// Records why an operation stopped.
    ///
    /// A failure to write this is swallowed: the original error is what the
    /// caller needs, and replacing it with a journal error would hide why the
    /// operation stopped in the first place. Recovery finds the record either way.
    fn settle(&mut self, record: &OperationRecord, stage: Stage, error: &Error) {
        let _ = self
            .journal
            .mark(record.id, stage, None, Some(error.code().as_str()));
    }
}
