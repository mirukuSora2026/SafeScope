//! Policy: what may be changed, how, and within what limits.
//!
//! The engine never reads the editable `policy.toml` directly. It reads an
//! approved snapshot, so editing the file grants nothing until a person approves
//! it at a terminal.

pub mod defaults;
pub mod evaluate;
pub mod file;
pub mod grant;
pub mod matcher;
pub mod normalized;
pub mod protected;
pub mod validate;

pub use self::defaults::SCHEMA_VERSION;
pub use self::evaluate::{
    Authority, Decision, EvaluationContext, MoveDecision, RuleRef, RuleSource, evaluate,
    evaluate_move,
};
pub use self::file::{ConflictAction, EnforcementMode, PolicyDocument, PolicyFile};
pub use self::grant::{ApprovalSource, Grant};
pub use self::matcher::{CaseSensitivity, Pattern, PatternSet};
pub use self::normalized::starter_policy_text;
pub use self::normalized::{
    AllowEntry, BudgetLimits, CompiledPolicy, DenyEntry, EnforcementSettings, NormalizedPolicy,
    RecoverySettings,
};
pub use self::protected::{ProtectedMatch, ProtectedPaths, ProtectedReason};
pub use self::validate::{Diagnostic, Severity, ValidationReport, validate};

use serde::{Deserialize, Serialize};

/// A monotonically increasing approved-policy version.
///
/// A counter rather than a content hash, because the engine needs to compare
/// versions for order — "was this plan made against an older policy?" — not just
/// for equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PolicyVersion(u64);

impl PolicyVersion {
    /// The version assigned by the first approval.
    pub const FIRST: Self = Self(1);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The version the next approval will be given.
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl std::fmt::Display for PolicyVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
