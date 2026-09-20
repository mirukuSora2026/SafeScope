//! Reading a workspace without taking it.
//!
//! Status, history and diagnostics are questions, not changes, and a question
//! that could not be asked while the MCP server was running would be a question
//! nobody could ask when it mattered. An [`Inspector`] therefore takes no lock.
//!
//! The division is the point: [`crate::session::WriteSession`] holds the lock
//! and can change things; this can do neither. Nothing here opens a path for
//! writing, so there is no way for a read to turn into a write by mistake.

use std::path::Path;

use crate::budget::{Budget, BudgetUsage};
use crate::dataformatting::Msg;
use crate::error::Result;
use crate::hash::ContentHash;
use crate::ids::TaskId;
use crate::journal::{Journal, OperationRecord};
use crate::policy::{AllowEntry, BudgetLimits, CompiledPolicy, Grant, PolicyVersion};
use crate::registry::{self, Registration};
use crate::store::StatePaths;
use crate::store::content::ContentStore;
use crate::store::grant_store::GrantStore;
use crate::store::policy_store::PolicyStore;
use crate::store::task_store::TaskStore;

/// A read-only view of a registered workspace.
#[derive(Debug)]
pub struct Inspector {
    registration: Registration,
    paths: StatePaths,
    policy: Option<CompiledPolicy>,
    policy_version: Option<PolicyVersion>,
    journal: Journal,
    snapshots: ContentStore,
    grants: GrantStore,
    task: Option<TaskId>,
}

impl Inspector {
    /// Opens a workspace for reading.
    ///
    /// Unlike a session this tolerates a workspace with no approved policy and
    /// no task: those are things a person may well be asking *about*.
    pub fn open(root: &Path) -> Result<Self> {
        let registration = registry::load(root)?;
        let paths = registration.state_paths()?;
        let approved = PolicyStore::new(&paths).current()?;

        Ok(Self {
            journal: Journal::open(&paths)?,
            snapshots: ContentStore::snapshots(&paths),
            grants: GrantStore::new(&paths),
            task: TaskStore::new(&paths).current()?,
            policy_version: approved.as_ref().map(|approved| approved.version),
            policy: approved
                .map(|approved| CompiledPolicy::compile(approved.policy))
                .transpose()?,
            registration,
            paths,
        })
    }

    pub const fn registration(&self) -> &Registration {
        &self.registration
    }

    pub const fn paths(&self) -> &StatePaths {
        &self.paths
    }

    pub const fn journal(&self) -> &Journal {
        &self.journal
    }

    pub const fn task(&self) -> Option<TaskId> {
        self.task
    }

    pub const fn policy(&self) -> Option<&CompiledPolicy> {
        self.policy.as_ref()
    }

    /// Everything the status screen needs.
    pub fn status(&self) -> Result<WorkspaceStatus> {
        let unsettled = self.journal.unsettled()?;
        let (usage, limits, allowed) = match (&self.policy, self.task) {
            (Some(policy), Some(task)) => (
                Budget {
                    limits: policy.budget(),
                    journal: &self.journal,
                    snapshots: &self.snapshots,
                }
                .usage(task)?,
                Some(*policy.budget()),
                policy.normalized().allow.clone(),
            ),
            (Some(policy), None) => (
                BudgetUsage::default(),
                Some(*policy.budget()),
                policy.normalized().allow.clone(),
            ),
            (None, _) => (BudgetUsage::default(), None, Vec::new()),
        };

        Ok(WorkspaceStatus {
            task: self.task,
            policy_version: self.policy_version,
            allowed,
            limits,
            usage,
            needs_attention: unsettled
                .iter()
                .filter(|record| record.stage.needs_attention())
                .count(),
            unsettled: unsettled.len(),
            last_operation: match self.task {
                Some(task) => self.journal.history(task)?.pop(),
                None => None,
            },
            unapproved_policy_edits: self.policy_edited(),
            grants: self.usable_grants()?.len(),
        })
    }

    /// A task's operations, most recent first.
    pub fn history(&self, limit: usize) -> Result<Vec<OperationRecord>> {
        let Some(task) = self.task else {
            return Ok(Vec::new());
        };
        let mut history = self.journal.history(task)?;
        history.reverse();
        history.truncate(limit);
        Ok(history)
    }

    /// What is worth telling somebody, if anything.
    ///
    /// Silence when there is nothing to say: a line that appears every session
    /// is a line nobody reads.
    pub fn concerns(&self) -> Result<Vec<Msg>> {
        let mut notes = Vec::new();
        let unsettled = self.journal.unsettled()?;

        if !unsettled.is_empty() {
            let attention = unsettled
                .iter()
                .filter(|record| record.stage.needs_attention())
                .count();
            if attention > 0 {
                notes.push(Msg::HookNeedsAttention { count: attention });
            }
            notes.push(Msg::HookUnsettledWork {
                count: unsettled.len(),
            });
        }
        if self.policy_edited() {
            notes.push(Msg::HookPolicyEdited);
        }
        Ok(notes)
    }

    fn usable_grants(&self) -> Result<Vec<Grant>> {
        match (self.task, self.policy_version) {
            (Some(task), Some(version)) => {
                self.grants
                    .usable(task, version, std::time::SystemTime::now())
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Whether the policy file has been edited since it was approved.
    ///
    /// The case this exists for: somebody changes the policy, does not approve
    /// it, and believes the change is in force.
    fn policy_edited(&self) -> bool {
        let Ok(Some(approved)) = PolicyStore::new(&self.paths).current() else {
            return false;
        };
        self.registration
            .read_policy_text()
            .is_ok_and(|text| approved.source_hash != ContentHash::of_bytes(text.as_bytes()))
    }
}

/// What a workspace can say about itself.
#[derive(Debug, Clone)]
pub struct WorkspaceStatus {
    /// Absent when no task has been started.
    pub task: Option<TaskId>,
    /// Absent when no policy has been approved.
    pub policy_version: Option<PolicyVersion>,
    pub allowed: Vec<AllowEntry>,
    pub limits: Option<BudgetLimits>,
    pub usage: BudgetUsage,
    /// Operations that have not reached a settled stage.
    pub unsettled: usize,
    /// Of those, the ones a person has to compare.
    pub needs_attention: usize,
    pub last_operation: Option<OperationRecord>,
    pub unapproved_policy_edits: bool,
    pub grants: usize,
}

impl WorkspaceStatus {
    /// Whether anything here should stop somebody trusting the workspace.
    pub const fn is_settled(&self) -> bool {
        self.unsettled == 0 && !self.unapproved_policy_edits
    }
}
