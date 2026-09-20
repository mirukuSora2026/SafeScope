//! A writing session: everything one caller needs, held together and locked.
//!
//! The planner reads the budget and the executor reserves it; the planner reads
//! a file's state and the executor acts on it. Both pairs are only sound if
//! nothing comes between them, and this is where that is arranged: a session
//! holds the workspace lock for as long as it lives, so there is one writer.
//!
//! It is also the only place that assembles the pieces. Somewhere has to know
//! that the executor's stores must be the planner's stores and that both must
//! belong to this workspace; leaving that to each caller is leaving it to be got
//! wrong once.
//!
//! The lock is advisory and scoped to SafeScope. It coordinates SafeScope
//! processes with each other and says nothing about an editor or a shell command
//! writing the same files.

use std::path::Path;

use crate::budget::{Budget, BudgetUsage};
use crate::dataformatting::Msg;
use crate::error::{Denial, Error, ErrorCode, Result};
use crate::executor::{Executor, RequestKey};
use crate::ids::TaskId;
use crate::journal::{Journal, OperationKind, OperationRecord};
use crate::path_guard::Workspace;
use crate::planner::{ChangePlan, ChangeRequest, Planner};
use crate::policy::{
    AllowEntry, Authority, BudgetLimits, CompiledPolicy, EvaluationContext, Grant, PolicyVersion,
};
use crate::recovery::{Recovery, RecoveryReport};
use crate::registry::{self, Registration};
use crate::store::content::ContentStore;
use crate::store::grant_store::GrantStore;
use crate::store::lock::WorkspaceLock;
use crate::store::policy_store::PolicyStore;
use crate::store::task_store::TaskStore;
use crate::undo::{Undo, UndoPlan};

/// Everything a caller needs to change files in one workspace.
#[derive(Debug)]
pub struct WriteSession {
    /// Held for the session's lifetime. Dropping the session releases it.
    _lock: WorkspaceLock,
    registration: Registration,
    workspace: Workspace,
    policy: CompiledPolicy,
    policy_version: PolicyVersion,
    journal: Journal,
    snapshots: ContentStore,
    staging: ContentStore,
    tasks: TaskStore,
    task: TaskId,
    grants: GrantStore,
    client: Option<ConnectedClient>,
    /// Approvals obtained through the client so far.
    ///
    /// Counted so a long run of small requests cannot quietly add up to a
    /// wide one: past a limit, further approvals have to be given at a
    /// terminal.
    elicitations: u32,
}

/// Who is driving this session.
///
/// Recorded at the one moment the client declares itself, and never inferred
/// afterwards. Whether it can put a question to a person decides which
/// approvals the engine may accept, so guessing is not an option.
#[derive(Debug, Clone)]
pub struct ConnectedClient {
    pub name: String,
    /// Declared during initialize. A client that did not declare it cannot be
    /// asked, so approvals through it are not available at all.
    pub can_ask_a_person: bool,
}

impl WriteSession {
    /// Opens a session on a registered workspace, taking the lock.
    ///
    /// Fails if another SafeScope process holds it, and if no policy has been
    /// approved — an unapproved workspace permits nothing, and saying so here is
    /// more useful than refusing every operation one at a time.
    pub fn open(root: &Path) -> Result<Self> {
        let registration = registry::load(root)?;
        let paths = registration.state_paths()?;
        paths.create()?;

        // Taken before anything is read, so nothing observed here can be stale
        // by the time it is used.
        let lock = WorkspaceLock::acquire(&paths)?;

        let approved = PolicyStore::new(&paths).current()?.ok_or_else(|| {
            Error::Denied(
                Denial::new(
                    ErrorCode::NoApprovedPolicy,
                    Msg::Label(crate::dataformatting::Label::Nothing),
                )
                .with_hint(Msg::HintFillInAllowThenApprove {
                    policy: registration.policy_path().display().to_string(),
                }),
            )
        })?;

        let tasks = TaskStore::new(&paths);
        let task = tasks.current_or_start()?;

        Ok(Self {
            _lock: lock,
            workspace: Workspace::open(root)?,
            policy: CompiledPolicy::compile(approved.policy)?,
            policy_version: approved.version,
            journal: Journal::open(&paths)?,
            snapshots: ContentStore::snapshots(&paths),
            staging: ContentStore::staging(&paths),
            tasks,
            task,
            grants: GrantStore::new(&paths),
            client: None,
            elicitations: 0,
            registration,
        })
    }

    pub const fn task(&self) -> TaskId {
        self.task
    }

    pub const fn policy_version(&self) -> PolicyVersion {
        self.policy_version
    }

    pub const fn policy(&self) -> &CompiledPolicy {
        &self.policy
    }

    pub const fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Temporary approvals in force right now.
    ///
    /// Read from the store on each call rather than cached, so an approval
    /// issued at a terminal while this session runs is visible immediately —
    /// which is the whole point of the terminal path.
    pub fn grants(&self) -> Result<Vec<Grant>> {
        self.grants
            .usable(self.task, self.policy_version, std::time::SystemTime::now())
    }

    /// Records who connected and what they can do.
    ///
    /// Called once, from the MCP handshake. It is not a request the client can
    /// repeat to change the answer later.
    pub fn note_client(&mut self, name: String, can_ask_a_person: bool) {
        self.client = Some(ConnectedClient {
            name,
            can_ask_a_person,
        });
    }

    pub fn client(&self) -> Option<&ConnectedClient> {
        self.client.as_ref()
    }

    /// How many approvals this task has collected through the client.
    pub const fn elicitations(&self) -> u32 {
        self.elicitations
    }

    /// Records that one more was collected.
    pub fn note_elicitation(&mut self) {
        self.elicitations += 1;
    }

    /// Records an approval obtained through the approval flow.
    ///
    /// Only the approval flow calls this. A grant that arrived any other way is
    /// not evidence of anything (I6).
    pub fn admit_grant(&self, grant: &Grant) -> Result<()> {
        self.grants.issue(grant)
    }

    /// Checks a request and stages everything it will need.
    pub fn plan(&self, request: &ChangeRequest) -> Result<ChangePlan> {
        Planner {
            workspace: &self.workspace,
            policy: &self.policy,
            journal: &self.journal,
            snapshots: &self.snapshots,
            staging: &self.staging,
        }
        .plan(request, &self.context(&self.grants()?))
    }

    /// Carries out a plan this session prepared.
    pub fn apply(
        &mut self,
        plan: &ChangePlan,
        request: Option<RequestKey>,
    ) -> Result<OperationRecord> {
        let limits = *self.policy.budget();
        Executor {
            workspace: &self.workspace,
            journal: &mut self.journal,
            limits: &limits,
            snapshots: &self.snapshots,
            staging: &self.staging,
        }
        .apply(plan, request)
    }

    /// Prepares a reversal of the most recent completed operation.
    pub fn prepare_undo(&self) -> Result<UndoPlan> {
        Undo {
            workspace: &self.workspace,
            policy: &self.policy,
            journal: &self.journal,
            snapshots: &self.snapshots,
            staging: &self.staging,
        }
        .prepare(self.task, self.policy_version)
    }

    /// Carries out a prepared reversal.
    ///
    /// Through the ordinary executor, marked as an undo so it does not spend the
    /// change budget and so a later undo does not reverse it in turn.
    pub fn apply_undo(&mut self, undo: &UndoPlan) -> Result<OperationRecord> {
        let limits = *self.policy.budget();
        Executor {
            workspace: &self.workspace,
            journal: &mut self.journal,
            limits: &limits,
            snapshots: &self.snapshots,
            staging: &self.staging,
        }
        .apply_reversing(&undo.plan, OperationKind::Undo, Some(undo.reverses), None)
    }

    /// Reconciles the journal with the workspace after a crash.
    pub fn recover(&mut self) -> Result<RecoveryReport> {
        Recovery {
            workspace: &self.workspace,
            journal: &mut self.journal,
        }
        .run()
    }

    /// What this session can say about itself.
    pub fn status(&self) -> Result<SessionStatus> {
        let usage = Budget {
            limits: self.policy.budget(),
            journal: &self.journal,
            snapshots: &self.snapshots,
        }
        .usage(self.task)?;

        let unsettled = self.journal.unsettled()?;
        let policy_edited = match PolicyStore::new(&self.registration.state_paths()?).current()? {
            Some(approved) => self
                .registration
                .read_policy_text()
                .map(|text| {
                    approved.source_hash != crate::hash::ContentHash::of_bytes(text.as_bytes())
                })
                .unwrap_or(false),
            None => false,
        };

        Ok(SessionStatus {
            task: self.task,
            policy_version: self.policy_version,
            allowed: self.policy.normalized().allow.clone(),
            limits: *self.policy.budget(),
            usage,
            needs_attention: unsettled
                .iter()
                .filter(|record| record.stage.needs_attention())
                .count(),
            unsettled: unsettled.len(),
            last_operation: self.journal.history(self.task)?.pop(),
            unapproved_policy_edits: policy_edited,
            grants: self.grants()?.len(),
        })
    }

    /// Ends the task, so the next session starts with a fresh budget.
    pub fn finish(&mut self) -> Result<()> {
        self.tasks.finish()?;
        self.task = self.tasks.current_or_start()?;
        Ok(())
    }

    fn context<'a>(&self, grants: &'a [Grant]) -> EvaluationContext<'a> {
        EvaluationContext {
            grants,
            task: self.task,
            policy_version: self.policy_version,
            now: std::time::SystemTime::now(),
            authority: Authority::Requested,
        }
    }
}

/// What a session reports about itself.
#[derive(Debug, Clone)]
pub struct SessionStatus {
    pub task: TaskId,
    pub policy_version: PolicyVersion,
    pub allowed: Vec<AllowEntry>,
    pub limits: BudgetLimits,
    pub usage: BudgetUsage,
    /// Operations that have not reached a settled stage.
    pub unsettled: usize,
    /// Of those, the ones a person has to compare.
    pub needs_attention: usize,
    pub last_operation: Option<OperationRecord>,
    /// Whether the policy file has been edited since it was approved.
    pub unapproved_policy_edits: bool,
    pub grants: usize,
}
