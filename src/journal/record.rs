//! What the journal stores about one operation.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::domain::{PathState, Transition};
use crate::hash::ContentHash;
use crate::ids::{OperationId, PlanId, RequestId, TaskId};

/// How far an operation got.
///
/// Deliberately not collapsed into "done" and "failed". After a crash the engine
/// has to distinguish "we know it did not run" from "we cannot tell", and a
/// two-state model would force it to guess — which is how a change gets applied
/// twice or an intact file gets restored over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// The plan is recorded and the payload staged. Nothing has been touched.
    Prepared,
    /// The file is about to change. This record is durable *before* it does,
    /// which is what makes recovery able to ask the right question afterwards.
    Applying,
    /// The change happened and the resulting state was observed.
    Committed,
    /// Confirmed not to have run; the reservation can be released.
    Aborted,
    /// Rejected before execution by policy, budget or a state mismatch.
    Rejected,
    /// What is on disk matches neither side of the plan.
    Conflict,
    /// Partly one side and partly the other. A person has to compare.
    RecoveryRequired,
}

impl Stage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Stage::Prepared => "prepared",
            Stage::Applying => "applying",
            Stage::Committed => "committed",
            Stage::Aborted => "aborted",
            Stage::Rejected => "rejected",
            Stage::Conflict => "conflict",
            Stage::RecoveryRequired => "recovery_required",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        [
            Stage::Prepared,
            Stage::Applying,
            Stage::Committed,
            Stage::Aborted,
            Stage::Rejected,
            Stage::Conflict,
            Stage::RecoveryRequired,
        ]
        .into_iter()
        .find(|stage| stage.as_str() == text)
    }

    /// Whether nothing more will happen to this operation.
    pub const fn is_settled(self) -> bool {
        matches!(
            self,
            Stage::Committed | Stage::Aborted | Stage::Rejected | Stage::Conflict
        )
    }

    /// Whether this operation needs looking at before the workspace is usable.
    pub const fn needs_attention(self) -> bool {
        matches!(self, Stage::Conflict | Stage::RecoveryRequired)
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One operation, as the journal holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationRecord {
    pub id: OperationId,
    pub task: TaskId,
    pub plan: PlanId,
    /// The caller's idempotency key, when one was given.
    pub request: Option<RequestId>,
    /// Hash of the request that produced this operation, so a resend can be
    /// told from a different request that reused the key.
    pub request_digest: Option<ContentHash>,
    /// Position in this workspace's order of operations.
    ///
    /// A counter rather than a timestamp: undo walks backwards, and a clock that
    /// moves backwards would lose the order.
    pub sequence: u64,
    pub stage: Stage,
    pub transition: Transition,
    /// Hash of the staged payload, for operations that have one.
    pub payload: Option<ContentHash>,
    /// State observed after the change, recorded at commit.
    pub observed: Option<Vec<PathState>>,
    /// Why it was rejected or could not be settled.
    pub error_code: Option<String>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

impl OperationRecord {
    /// Whether this operation may still be undone.
    ///
    /// Only a committed operation: one that never ran has nothing to undo, and
    /// one whose outcome is unclear must be compared by a person first.
    pub const fn is_undoable(&self) -> bool {
        matches!(self.stage, Stage::Committed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_names_round_trip() {
        let stages = [
            Stage::Prepared,
            Stage::Applying,
            Stage::Committed,
            Stage::Aborted,
            Stage::Rejected,
            Stage::Conflict,
            Stage::RecoveryRequired,
        ];
        for stage in stages {
            assert_eq!(Stage::parse(stage.as_str()), Some(stage));
        }
        assert_eq!(Stage::parse("half-done"), None);
    }

    #[test]
    fn applying_is_not_settled() {
        // The whole point of the stage: after a crash an operation left here has
        // to be compared against the filesystem, not assumed either way.
        assert!(!Stage::Applying.is_settled());
        assert!(!Stage::Prepared.is_settled());
        assert!(Stage::Committed.is_settled());
        assert!(Stage::Aborted.is_settled());
    }

    #[test]
    fn unclear_outcomes_need_attention() {
        assert!(Stage::Conflict.needs_attention());
        assert!(Stage::RecoveryRequired.needs_attention());
        assert!(!Stage::Committed.needs_attention());
        assert!(!Stage::Aborted.needs_attention());
    }
}
