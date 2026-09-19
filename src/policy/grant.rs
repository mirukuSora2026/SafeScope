//! Temporary approvals.
//!
//! A grant is deliberately unlike a policy rule, and the differences are the
//! design:
//!
//! | | Policy rule | Grant |
//! |---|---|---|
//! | shape | glob pattern | **exact paths only** |
//! | lifetime | permanent | a time to live, consumed once |
//! | stored | project file, approvable | engine state only |
//! | author | a person editing and approving | a person answering a request |
//!
//! Glob patterns are refused here on purpose. Asked for one file, a client could
//! propose `src/**` as "the pattern for it"; exact paths mean a request opens
//! precisely what was asked for, and several files mean several grants, which a
//! person sees as a count.
//!
//! A grant also carries the policy version it was issued against, so editing and
//! re-approving a policy invalidates every outstanding grant.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::domain::{OpSet, Operation};
use crate::ids::{GrantId, PlanId, TaskId};
use crate::paths::RelPath;

use super::PolicyVersion;

/// Where an approval came from.
///
/// Recorded rather than collapsed into a boolean because the status output has
/// to be able to say how a task's approvals were obtained. An approval that
/// arrived over the client channel is weaker evidence than one typed at a
/// terminal, and hiding that difference would overstate what the engine knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "via", rename_all = "snake_case")]
pub enum ApprovalSource {
    /// Answered through the MCP client. The client's identity is recorded.
    Elicitation { client: String, session: String },
    /// Typed by a person at a terminal.
    Terminal,
}

impl ApprovalSource {
    /// Whether this source is a person demonstrably at the machine.
    pub const fn is_terminal(&self) -> bool {
        matches!(self, ApprovalSource::Terminal)
    }
}

/// A temporary approval covering exact paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    pub id: GrantId,
    pub task: TaskId,
    /// The approved policy version this was issued against.
    pub policy_version: PolicyVersion,
    /// Exact paths. Never patterns.
    pub paths: Vec<RelPath>,
    pub ops: OpSet,
    pub issued_by: ApprovalSource,
    pub expires_at: SystemTime,
    /// The plan that spent this grant, if any.
    pub consumed_by: Option<PlanId>,
}

impl Grant {
    /// Builds a grant. Callers come from the approval flow, never from tool input.
    pub fn new(
        task: TaskId,
        policy_version: PolicyVersion,
        paths: Vec<RelPath>,
        ops: OpSet,
        issued_by: ApprovalSource,
        expires_at: SystemTime,
    ) -> Self {
        Self {
            id: GrantId::new(),
            task,
            policy_version,
            paths,
            ops,
            issued_by,
            expires_at,
            consumed_by: None,
        }
    }

    /// Whether this grant can still be used.
    ///
    /// All four conditions are checked together so no caller can accidentally
    /// test three of them.
    pub fn is_usable(&self, task: TaskId, policy_version: PolicyVersion, now: SystemTime) -> bool {
        self.consumed_by.is_none()
            && self.task == task
            && self.policy_version == policy_version
            && now < self.expires_at
    }

    /// Whether this grant covers a path and operation, ignoring usability.
    pub fn covers(&self, path: &RelPath, operation: Operation) -> bool {
        self.ops.contains(operation) && self.paths.iter().any(|granted| granted == path)
    }

    /// Marks the grant as spent.
    pub fn consume(&mut self, plan: PlanId) {
        self.consumed_by = Some(plan);
    }

    pub const fn is_consumed(&self) -> bool {
        self.consumed_by.is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn path(text: &str) -> RelPath {
        RelPath::parse(text).unwrap()
    }

    fn grant_for(task: TaskId, version: PolicyVersion, now: SystemTime) -> Grant {
        Grant::new(
            task,
            version,
            vec![path("src/auth/Helper.java")],
            [Operation::Replace].into_iter().collect(),
            ApprovalSource::Terminal,
            now + Duration::from_secs(1800),
        )
    }

    #[test]
    fn covers_the_exact_path_and_operation_it_was_issued_for() {
        let now = SystemTime::now();
        let grant = grant_for(TaskId::new(), PolicyVersion::FIRST, now);

        assert!(grant.covers(&path("src/auth/Helper.java"), Operation::Replace));
        assert!(!grant.covers(&path("src/auth/Helper.java"), Operation::Trash));
        assert!(!grant.covers(&path("src/auth/Other.java"), Operation::Replace));
    }

    #[test]
    fn does_not_cover_a_neighbouring_path() {
        // The reason grants hold exact paths: asked for one file, a client
        // cannot widen the request into a directory.
        let now = SystemTime::now();
        let grant = grant_for(TaskId::new(), PolicyVersion::FIRST, now);
        assert!(!grant.covers(&path("src/auth/token/Helper.java"), Operation::Replace));
        assert!(!grant.covers(&path("src/auth"), Operation::Replace));
    }

    #[test]
    fn expires() {
        let now = SystemTime::now();
        let task = TaskId::new();
        let grant = grant_for(task, PolicyVersion::FIRST, now);

        assert!(grant.is_usable(task, PolicyVersion::FIRST, now));
        assert!(!grant.is_usable(task, PolicyVersion::FIRST, now + Duration::from_secs(3600)));
    }

    #[test]
    fn is_invalidated_by_a_new_policy_version() {
        // Editing and re-approving a policy withdraws every outstanding grant.
        let now = SystemTime::now();
        let task = TaskId::new();
        let grant = grant_for(task, PolicyVersion::FIRST, now);

        assert!(grant.is_usable(task, PolicyVersion::FIRST, now));
        assert!(!grant.is_usable(task, PolicyVersion::FIRST.next(), now));
    }

    #[test]
    fn does_not_carry_across_tasks() {
        let now = SystemTime::now();
        let task = TaskId::new();
        let grant = grant_for(task, PolicyVersion::FIRST, now);
        assert!(!grant.is_usable(TaskId::new(), PolicyVersion::FIRST, now));
    }

    #[test]
    fn is_spent_once_used() {
        let now = SystemTime::now();
        let task = TaskId::new();
        let mut grant = grant_for(task, PolicyVersion::FIRST, now);

        assert!(grant.is_usable(task, PolicyVersion::FIRST, now));
        grant.consume(PlanId::new());
        assert!(grant.is_consumed());
        assert!(!grant.is_usable(task, PolicyVersion::FIRST, now));
    }

    #[test]
    fn records_how_the_approval_was_obtained() {
        // The status output has to be able to say this, so it is never collapsed
        // into a boolean.
        let elicited = ApprovalSource::Elicitation {
            client: "claude-code".to_owned(),
            session: "abc".to_owned(),
        };
        assert!(!elicited.is_terminal());
        assert!(ApprovalSource::Terminal.is_terminal());
    }

    #[test]
    fn round_trips_through_json() {
        let now = SystemTime::now();
        let grant = grant_for(TaskId::new(), PolicyVersion::FIRST, now);
        let json = serde_json::to_string(&grant).unwrap();
        assert_eq!(serde_json::from_str::<Grant>(&json).unwrap(), grant);
    }

    #[test]
    fn a_stored_grant_with_an_invalid_path_is_rejected() {
        // Grants live in engine state, but a damaged record must not reach
        // evaluation with an unchecked path.
        let json = r#"{
            "id": "00000000-0000-4000-8000-000000000001",
            "task": "00000000-0000-4000-8000-000000000002",
            "policy_version": 1,
            "paths": ["../escape"],
            "ops": ["replace"],
            "issued_by": {"via": "terminal"},
            "expires_at": {"secs_since_epoch": 0, "nanos_since_epoch": 0},
            "consumed_by": null
        }"#;
        assert!(serde_json::from_str::<Grant>(json).is_err());
    }
}
