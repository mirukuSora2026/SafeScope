//! Deciding whether one operation on one path is permitted.
//!
//! The order is fixed and every step is explainable, because `safescope check`
//! has to be able to show a person why a path was refused:
//!
//! ```text
//! 1. protected (built in)  → refuse; no approval can lift it
//! 2. deny (policy)         → refuse; no approval can lift it
//! 3. allow (policy)        → permit, or refuse this operation
//! 4. grant (temporary)     → permit
//! 5. nothing matched       → not covered; an expansion may be requested
//! ```
//!
//! Two consequences of that order are deliberate.
//!
//! **A grant cannot override a deny.** Allowing it would mean every deny rule
//! has to be re-read as "deny, unless someone approves otherwise", and a rule
//! that conditional is not a rule. A real exception means editing the policy.
//!
//! **Allow rules union rather than shadow.** If one rule matches a path without
//! permitting the operation and a later rule permits it, the operation is
//! allowed: an allow list is a set of grants, not a sequence of overrides. The
//! operation is refused only when a rule matches the path and none permits it.

use std::time::SystemTime;

use crate::dataformatting::Msg;
use crate::domain::{OpSet, Operation};
use crate::error::{Denial, ErrorCode};
use crate::ids::{GrantId, TaskId};
use crate::paths::RelPath;

use super::grant::Grant;
use super::normalized::CompiledPolicy;
use super::protected::{self, ProtectedReason};
use super::{PolicyVersion, matcher::Pattern};

/// Which kind of rule produced a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleSource {
    /// A path the engine always protects.
    Protected(ProtectedReason),
    /// A policy deny rule.
    PolicyDeny,
    /// A policy allow rule.
    PolicyAllow,
    /// A temporary approval.
    Grant(GrantId),
}

/// Enough to point a person at the rule responsible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleRef {
    pub source: RuleSource,
    /// The pattern text, absent for grants and for the reserved-name rule.
    pub pattern: Option<String>,
    /// Line in the policy file, when there is one.
    pub line: Option<u32>,
}

impl RuleRef {
    fn from_pattern(source: RuleSource, pattern: &Pattern) -> Self {
        Self {
            source,
            pattern: Some(pattern.text().to_owned()),
            line: pattern.line(),
        }
    }
}

/// The outcome of evaluating one operation on one path.
#[derive(Debug, Clone)]
pub enum Decision {
    /// Permitted by the given rule.
    Allow { rule: RuleRef },
    /// Refused. `rule` is absent when nothing in particular did the refusing.
    Deny {
        rule: Option<RuleRef>,
        denial: Denial,
    },
    /// No rule covers this path. The only outcome from which a scope expansion
    /// may be requested.
    NotCovered { denial: Denial },
}

impl Decision {
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Decision::Allow { .. })
    }

    /// Whether asking the user to widen the scope could change this answer.
    ///
    /// A protected path or a deny rule is final; only an uncovered path or a
    /// missing operation can be opened by an approval.
    pub fn may_request_expansion(&self) -> bool {
        match self {
            Decision::Allow { .. } => false,
            Decision::NotCovered { .. } => true,
            Decision::Deny { denial, .. } => denial.code() == ErrorCode::OperationNotAllowed,
        }
    }

    /// The rule responsible, when there is one.
    pub const fn rule(&self) -> Option<&RuleRef> {
        match self {
            Decision::Allow { rule } => Some(rule),
            Decision::Deny { rule, .. } => rule.as_ref(),
            Decision::NotCovered { .. } => None,
        }
    }

    /// Collapses into a result, which is what callers that only need yes or no use.
    pub fn into_result(self) -> Result<RuleRef, Denial> {
        match self {
            Decision::Allow { rule } => Ok(rule),
            Decision::Deny { denial, .. } | Decision::NotCovered { denial } => Err(denial),
        }
    }
}

/// What evaluation needs beyond the policy itself.
#[derive(Debug, Clone, Copy)]
pub struct EvaluationContext<'a> {
    /// Outstanding approvals. Usability is checked here, not by the caller.
    pub grants: &'a [Grant],
    pub task: TaskId,
    pub policy_version: PolicyVersion,
    pub now: SystemTime,
}

/// Decides whether `operation` may be performed on `path`.
pub fn evaluate(
    path: &RelPath,
    operation: Operation,
    policy: &CompiledPolicy,
    context: &EvaluationContext<'_>,
) -> Decision {
    if let Some(found) = policy.protected().find(path) {
        return Decision::Deny {
            rule: Some(RuleRef {
                source: RuleSource::Protected(found.reason),
                pattern: found.pattern.map(|pattern| pattern.text().to_owned()),
                line: None,
            }),
            denial: protected::denial(found.reason),
        };
    }

    if let Some(pattern) = policy.deny().find(path) {
        return Decision::Deny {
            rule: Some(RuleRef::from_pattern(RuleSource::PolicyDeny, pattern)),
            denial: Denial::new(
                ErrorCode::ScopeDenied,
                Msg::ScopeDeniedByRule {
                    path: path.as_str().to_owned(),
                    pattern: pattern.text().to_owned(),
                },
            )
            .with_hint(Msg::HintPolicyDenyIsFinal),
        };
    }

    let matching: Vec<_> = policy
        .allow()
        .iter()
        .filter(|rule| rule.pattern.matches(path))
        .collect();

    if let Some(rule) = matching.iter().find(|rule| rule.ops.contains(operation)) {
        return Decision::Allow {
            rule: RuleRef::from_pattern(RuleSource::PolicyAllow, &rule.pattern),
        };
    }

    if let Some(first) = matching.first() {
        // The path is in scope, but this operation is not. That is a narrower
        // refusal than "out of scope", and an expansion can still open it.
        let allowed: OpSet = matching
            .iter()
            .map(|rule| rule.ops)
            .fold(OpSet::empty(), |combined, ops| {
                ops.iter().fold(combined, OpSet::with)
            });
        return Decision::Deny {
            rule: Some(RuleRef::from_pattern(
                RuleSource::PolicyAllow,
                &first.pattern,
            )),
            denial: Denial::new(
                ErrorCode::OperationNotAllowed,
                Msg::ScopeOperationNotAllowed {
                    path: path.as_str().to_owned(),
                    operation: operation.to_string(),
                    allowed: allowed.to_string(),
                },
            )
            .with_hint(Msg::HintExpansionMayBeRequested),
        };
    }

    if let Some(grant) = context.grants.iter().find(|grant| {
        grant.is_usable(context.task, context.policy_version, context.now)
            && grant.covers(path, operation)
    }) {
        return Decision::Allow {
            rule: RuleRef {
                source: RuleSource::Grant(grant.id),
                pattern: None,
                line: None,
            },
        };
    }

    Decision::NotCovered {
        denial: Denial::new(
            ErrorCode::ScopeDenied,
            Msg::ScopeNotCovered {
                path: path.as_str().to_owned(),
            },
        )
        .with_hint(Msg::HintExpansionMayBeRequested),
    }
}

/// A move is evaluated at both ends.
///
/// Checking only the source would let a file be pushed out of the allowed scope
/// and into a forbidden one.
#[derive(Debug, Clone)]
pub struct MoveDecision {
    pub source: Decision,
    pub destination: Decision,
}

impl MoveDecision {
    pub const fn is_allowed(&self) -> bool {
        self.source.is_allowed() && self.destination.is_allowed()
    }

    /// The refusal to report, preferring the source end when both fail.
    pub fn into_result(self) -> Result<(), Denial> {
        self.source.into_result()?;
        self.destination.into_result()?;
        Ok(())
    }
}

/// Decides whether a file may be moved from one path to another.
pub fn evaluate_move(
    from: &RelPath,
    to: &RelPath,
    policy: &CompiledPolicy,
    context: &EvaluationContext<'_>,
) -> MoveDecision {
    MoveDecision {
        source: evaluate(from, Operation::Move, policy, context),
        destination: evaluate(to, Operation::Move, policy, context),
    }
}
