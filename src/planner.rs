//! Turning a request into a plan that `apply` can carry out.
//!
//! Planning does all the work that can be done without touching the workspace,
//! and it fixes the plan's contents completely. Three things follow from that,
//! and they are the reason the split exists at all:
//!
//! - `apply` takes nothing but a plan id, so the new contents cannot be
//!   substituted between deciding and doing.
//! - The *after* hash is known before anything happens, which is what lets crash
//!   recovery decide whether an operation ran instead of guessing.
//! - A destructive operation's snapshot is stored and verified here, so by the
//!   time anything is at risk the recovery data already exists (I2).
//!
//! A plan is a statement about a particular state of the workspace. It carries
//! the exact state it expects on both sides, and [`crate::executor`] refuses to
//! act on it if the workspace has moved on (I3).

use std::time::{Duration, SystemTime};

use crate::budget::Budget;
use crate::dataformatting::{self, Msg};
use crate::domain::{FileState, Operation, Transition};
use crate::error::{Denial, Error, ErrorCode, Result};
use crate::hash::ContentHash;
use crate::ids::{PlanId, TaskId};
use crate::journal::Journal;
use crate::path_guard::Workspace;
use crate::paths::RelPath;
use crate::policy::{CompiledPolicy, EvaluationContext, PolicyVersion, evaluate, evaluate_move};
use crate::store::content::ContentStore;

/// How long a plan stays applicable.
///
/// Long enough for a person to be asked and answer, short enough that a plan
/// built against contents nobody remembers does not linger. Applying an expired
/// plan is refused outright rather than re-checked, because a plan is a
/// statement about a moment.
pub const PLAN_LIFETIME: Duration = Duration::from_secs(30 * 60);

/// What a caller asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeRequest {
    Create { path: RelPath, contents: Vec<u8> },
    Replace { path: RelPath, contents: Vec<u8> },
    Trash { path: RelPath },
    Move { from: RelPath, to: RelPath },
}

impl ChangeRequest {
    pub const fn operation(&self) -> Operation {
        match self {
            ChangeRequest::Create { .. } => Operation::Create,
            ChangeRequest::Replace { .. } => Operation::Replace,
            ChangeRequest::Trash { .. } => Operation::Trash,
            ChangeRequest::Move { .. } => Operation::Move,
        }
    }

    /// The contents the request would write, if any.
    pub fn contents(&self) -> Option<&[u8]> {
        match self {
            ChangeRequest::Create { contents, .. } | ChangeRequest::Replace { contents, .. } => {
                Some(contents)
            }
            _ => None,
        }
    }
}

/// A checked, stored plan.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangePlan {
    pub id: PlanId,
    pub task: TaskId,
    pub policy_version: PolicyVersion,
    pub transition: Transition,
    /// The staged contents, for operations that write. Already durable.
    pub payload: Option<ContentHash>,
    pub created_at: SystemTime,
    pub expires_at: SystemTime,
}

impl ChangePlan {
    pub fn has_expired(&self, now: SystemTime) -> bool {
        now >= self.expires_at
    }

    /// The refusal for a plan that is past its time.
    pub fn expiry_denial(&self) -> Denial {
        Denial::new(
            ErrorCode::PlanExpired,
            Msg::PlanHasExpired {
                plan: self.id.to_string(),
            },
        )
        .with_hint(Msg::HintRebuildThePlan)
    }
}

/// Everything planning needs beyond the request.
pub struct Planner<'a> {
    pub workspace: &'a Workspace,
    pub policy: &'a CompiledPolicy,
    pub journal: &'a Journal,
    pub snapshots: &'a ContentStore,
    pub staging: &'a ContentStore,
}

impl Planner<'_> {
    /// Checks a request and turns it into a plan.
    ///
    /// Nothing in the workspace is modified. The only writes are to the engine's
    /// own stores: the payload, and — for a destructive operation — the snapshot
    /// that makes it undoable.
    pub fn plan(
        &self,
        request: &ChangeRequest,
        context: &EvaluationContext<'_>,
    ) -> Result<ChangePlan> {
        self.check_policy(request, context)?;

        // Observation only: nothing is written until the budget has agreed, so a
        // refusal here leaves no snapshot and no staged payload behind.
        let transition = self.build_transition(request)?;

        Budget {
            limits: self.policy.budget(),
            journal: self.journal,
            snapshots: self.snapshots,
        }
        .check(context.task, &transition)?;

        // I2: the previous contents become recoverable while the file is still
        // intact, not at the moment of overwriting.
        self.take_snapshot(&transition)?;

        // Staged before the plan is handed out, so `apply` needs nothing but an
        // id and recovery knows the resulting hash in advance.
        let payload = match request.contents() {
            Some(contents) => Some(self.staging.store(contents)?),
            None => None,
        };

        let created_at = SystemTime::now();
        Ok(ChangePlan {
            id: PlanId::new(),
            task: context.task,
            policy_version: context.policy_version,
            transition,
            payload,
            created_at,
            expires_at: created_at + PLAN_LIFETIME,
        })
    }

    fn check_policy(&self, request: &ChangeRequest, context: &EvaluationContext<'_>) -> Result<()> {
        match request {
            ChangeRequest::Move { from, to } => {
                evaluate_move(from, to, self.policy, context).into_result()?;
            }
            other => {
                let path = self.primary_path(other);
                evaluate(path, other.operation(), self.policy, context).into_result()?;
            }
        }
        Ok(())
    }

    fn primary_path<'r>(&self, request: &'r ChangeRequest) -> &'r RelPath {
        match request {
            ChangeRequest::Create { path, .. }
            | ChangeRequest::Replace { path, .. }
            | ChangeRequest::Trash { path } => path,
            ChangeRequest::Move { from, .. } => from,
        }
    }

    fn build_transition(&self, request: &ChangeRequest) -> Result<Transition> {
        match request {
            ChangeRequest::Create { path, contents } => {
                let resolved = self.workspace.resolve(path)?;
                if resolved.exists() {
                    return Err(exists(path));
                }
                self.check_size(path, contents.len() as u64)?;
                Ok(Transition::create(
                    path.clone(),
                    ContentHash::of_bytes(contents),
                    contents.len() as u64,
                ))
            }

            ChangeRequest::Replace { path, contents } => {
                let (hash, len) = self.require_present(path)?;
                self.check_size(path, len)?;
                self.check_size(path, contents.len() as u64)?;
                Ok(Transition::replace(
                    path.clone(),
                    (hash, len),
                    (ContentHash::of_bytes(contents), contents.len() as u64),
                ))
            }

            ChangeRequest::Trash { path } => {
                let (hash, len) = self.require_present(path)?;
                self.check_size(path, len)?;
                Ok(Transition::trash(path.clone(), hash, len))
            }

            ChangeRequest::Move { from, to } => {
                let (hash, len) = self.require_present(from)?;
                if self.workspace.resolve(to)?.exists() {
                    return Err(exists(to));
                }
                // A move preserves the contents, so there is nothing to snapshot:
                // undoing it is the reverse rename.
                Ok(Transition::rename(from.clone(), to.clone(), hash, len))
            }
        }
    }

    /// Reads the current contents of a file that must be there.
    fn require_present(&self, path: &RelPath) -> Result<(ContentHash, u64)> {
        match self.workspace.resolve(path)?.state() {
            FileState::Present { hash, len } => Ok((*hash, *len)),
            FileState::Absent => Err(Error::Denied(Denial::new(
                ErrorCode::TargetMissing,
                Msg::PlanTargetMissing {
                    path: path.as_str().to_owned(),
                },
            ))),
        }
    }

    /// Stores recovery data for whatever this transition will destroy.
    fn take_snapshot(&self, transition: &Transition) -> Result<()> {
        if !transition.operation().destroys_content() {
            // A move preserves its contents; undoing it is the reverse rename.
            return Ok(());
        }
        for state in transition.states(crate::domain::Phase::Before) {
            if let Some(hash) = state.state.hash() {
                self.snapshot(&state.path, *hash)?;
            }
        }
        Ok(())
    }

    /// Copies the current contents into the snapshot store and verifies them.
    fn snapshot(&self, path: &RelPath, expected: ContentHash) -> Result<()> {
        let resolved = self.workspace.resolve(path)?;
        let contents = resolved
            .parent()
            .read(resolved.file_name())
            .map_err(|error| Error::Faulted(crate::error::Fault::io("read for snapshot", error)))?;

        // The file changed between being measured and being read. Refusing here
        // keeps the snapshot and the plan describing the same contents.
        if ContentHash::of_bytes(&contents) != expected {
            return Err(changed(path));
        }
        self.snapshots.store(&contents).map(|_| ())
    }

    fn check_size(&self, path: &RelPath, size: u64) -> Result<()> {
        let limit = self.policy.budget().max_file_bytes;
        if size > limit {
            return Err(Error::Denied(Denial::new(
                ErrorCode::FileTooLarge,
                Msg::PlanFileTooLarge {
                    path: path.as_str().to_owned(),
                    size: dataformatting::bytes(size),
                    limit: dataformatting::bytes(limit),
                },
            )));
        }
        Ok(())
    }
}

fn exists(path: &RelPath) -> Error {
    Error::Denied(Denial::new(
        ErrorCode::DestinationExists,
        Msg::PlanTargetExists {
            path: path.as_str().to_owned(),
        },
    ))
}

/// The refusal raised when the workspace has moved on since planning.
pub fn state_changed(path: &RelPath) -> Error {
    changed(path)
}

fn changed(path: &RelPath) -> Error {
    Error::Denied(
        Denial::new(
            ErrorCode::SourceChanged,
            Msg::PlanStateChanged {
                path: path.as_str().to_owned(),
            },
        )
        .with_hint(Msg::HintRebuildThePlan),
    )
}
