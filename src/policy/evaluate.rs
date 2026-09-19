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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::ids::PlanId;
    use crate::policy::grant::ApprovalSource;
    use crate::policy::normalized::NormalizedPolicy;

    fn path(text: &str) -> RelPath {
        RelPath::parse(text).unwrap()
    }

    fn compiled(text: &str) -> CompiledPolicy {
        CompiledPolicy::compile(NormalizedPolicy::from_text(text).unwrap()).unwrap()
    }

    fn context<'a>(grants: &'a [Grant], task: TaskId) -> EvaluationContext<'a> {
        EvaluationContext {
            grants,
            task,
            policy_version: PolicyVersion::FIRST,
            now: SystemTime::now(),
        }
    }

    const POLICY: &str = "\
schema_version = 1

[scope]
allow = [\"src/main/java/auth/**\"]
deny = [\"**/.env\"]
default_ops = [\"create\", \"replace\", \"move\", \"trash\"]

[[scope.allow_rule]]
path = \"src/main/resources/**\"
ops = [\"replace\"]
";

    #[test]
    fn allows_a_path_in_scope() {
        let policy = compiled(POLICY);
        let decision = evaluate(
            &path("src/main/java/auth/Login.java"),
            Operation::Replace,
            &policy,
            &context(&[], TaskId::new()),
        );
        assert!(decision.is_allowed());
        let rule = decision.rule().expect("a rule");
        assert_eq!(rule.source, RuleSource::PolicyAllow);
        assert_eq!(rule.pattern.as_deref(), Some("src/main/java/auth/**"));
        assert_eq!(rule.line, Some(4), "the rule's line is carried through");
    }

    #[test]
    fn refuses_a_path_outside_every_rule() {
        let policy = compiled(POLICY);
        let decision = evaluate(
            &path("src/main/java/common/DateUtils.java"),
            Operation::Replace,
            &policy,
            &context(&[], TaskId::new()),
        );
        assert!(!decision.is_allowed());
        assert!(matches!(decision, Decision::NotCovered { .. }));
        assert!(decision.may_request_expansion());
    }

    #[test]
    fn a_protected_path_is_refused_and_cannot_be_expanded() {
        let policy = compiled(POLICY);
        let decision = evaluate(
            &path(".git/config"),
            Operation::Replace,
            &policy,
            &context(&[], TaskId::new()),
        );
        assert!(!decision.is_allowed());
        assert!(!decision.may_request_expansion());
        assert!(matches!(
            decision.rule().map(|rule| &rule.source),
            Some(RuleSource::Protected(_))
        ));
    }

    #[test]
    fn a_deny_rule_is_final() {
        let policy = compiled(POLICY);
        let decision = evaluate(
            &path("src/.env"),
            Operation::Replace,
            &policy,
            &context(&[], TaskId::new()),
        );
        assert!(!decision.is_allowed());
        assert!(
            !decision.may_request_expansion(),
            "a deny cannot be lifted by approval"
        );
        assert_eq!(
            decision.rule().map(|rule| &rule.source),
            Some(&RuleSource::PolicyDeny)
        );
    }

    #[test]
    fn refuses_an_operation_the_rule_does_not_grant() {
        let policy = compiled(POLICY);
        let decision = evaluate(
            &path("src/main/resources/messages.properties"),
            Operation::Trash,
            &policy,
            &context(&[], TaskId::new()),
        );
        assert!(!decision.is_allowed());
        assert!(
            decision.may_request_expansion(),
            "a missing operation is narrower than being out of scope"
        );
        let denial = decision.into_result().unwrap_err();
        assert_eq!(denial.code(), ErrorCode::OperationNotAllowed);
        assert!(denial.message().contains("replace"), "{}", denial.message());
    }

    #[test]
    fn allow_rules_union_rather_than_shadow() {
        // The first rule matches without granting trash; the second grants it.
        // An allow list is a set of grants, not a sequence of overrides.
        let policy = compiled(
            "schema_version = 1\n\
             \n\
             [[scope.allow_rule]]\n\
             path = \"src/**\"\n\
             ops = [\"replace\"]\n\
             \n\
             [[scope.allow_rule]]\n\
             path = \"src/scratch/**\"\n\
             ops = [\"trash\"]\n",
        );
        let decision = evaluate(
            &path("src/scratch/tmp.txt"),
            Operation::Trash,
            &policy,
            &context(&[], TaskId::new()),
        );
        assert!(decision.is_allowed());
        assert_eq!(
            decision.rule().and_then(|rule| rule.pattern.as_deref()),
            Some("src/scratch/**")
        );
    }

    #[test]
    fn a_grant_opens_an_uncovered_path() {
        let task = TaskId::new();
        let policy = compiled(POLICY);
        let target = path("src/main/java/common/DateUtils.java");
        let grants = vec![Grant::new(
            task,
            PolicyVersion::FIRST,
            vec![target.clone()],
            [Operation::Replace].into_iter().collect(),
            ApprovalSource::Terminal,
            SystemTime::now() + Duration::from_secs(600),
        )];

        let decision = evaluate(
            &target,
            Operation::Replace,
            &policy,
            &context(&grants, task),
        );
        assert!(decision.is_allowed());
        assert!(matches!(
            decision.rule().map(|rule| &rule.source),
            Some(RuleSource::Grant(_))
        ));
    }

    #[test]
    fn a_grant_cannot_override_a_deny() {
        // Otherwise every deny rule reads as "deny, unless someone approves
        // otherwise", and a rule that conditional is not a rule.
        let task = TaskId::new();
        let policy = compiled(POLICY);
        let target = path("src/.env");
        let grants = vec![Grant::new(
            task,
            PolicyVersion::FIRST,
            vec![target.clone()],
            OpSet::all(),
            ApprovalSource::Terminal,
            SystemTime::now() + Duration::from_secs(600),
        )];

        let decision = evaluate(
            &target,
            Operation::Replace,
            &policy,
            &context(&grants, task),
        );
        assert!(!decision.is_allowed());
        assert_eq!(
            decision.rule().map(|rule| &rule.source),
            Some(&RuleSource::PolicyDeny)
        );
    }

    #[test]
    fn a_grant_cannot_override_a_protected_path() {
        let task = TaskId::new();
        let policy = compiled(POLICY);
        let target = path(".claude/settings.json");
        let grants = vec![Grant::new(
            task,
            PolicyVersion::FIRST,
            vec![target.clone()],
            OpSet::all(),
            ApprovalSource::Terminal,
            SystemTime::now() + Duration::from_secs(600),
        )];

        let decision = evaluate(
            &target,
            Operation::Replace,
            &policy,
            &context(&grants, task),
        );
        assert!(!decision.is_allowed());
    }

    #[test]
    fn a_spent_grant_no_longer_opens_anything() {
        let task = TaskId::new();
        let policy = compiled(POLICY);
        let target = path("src/main/java/common/DateUtils.java");
        let mut grant = Grant::new(
            task,
            PolicyVersion::FIRST,
            vec![target.clone()],
            [Operation::Replace].into_iter().collect(),
            ApprovalSource::Terminal,
            SystemTime::now() + Duration::from_secs(600),
        );
        grant.consume(PlanId::new());

        let decision = evaluate(
            &target,
            Operation::Replace,
            &policy,
            &context(&[grant], task),
        );
        assert!(!decision.is_allowed());
    }

    #[test]
    fn a_grant_from_another_task_is_ignored() {
        let policy = compiled(POLICY);
        let target = path("src/main/java/common/DateUtils.java");
        let grants = vec![Grant::new(
            TaskId::new(),
            PolicyVersion::FIRST,
            vec![target.clone()],
            OpSet::all(),
            ApprovalSource::Terminal,
            SystemTime::now() + Duration::from_secs(600),
        )];

        let decision = evaluate(
            &target,
            Operation::Replace,
            &policy,
            &context(&grants, TaskId::new()),
        );
        assert!(!decision.is_allowed());
    }

    #[test]
    fn a_move_is_checked_at_both_ends() {
        let policy = compiled(POLICY);
        let context = context(&[], TaskId::new());

        let inside = evaluate_move(
            &path("src/main/java/auth/A.java"),
            &path("src/main/java/auth/B.java"),
            &policy,
            &context,
        );
        assert!(inside.is_allowed());

        // Allowed at the source, refused at the destination: without the second
        // check a file could be pushed out of scope.
        let escaping = evaluate_move(
            &path("src/main/java/auth/A.java"),
            &path("src/main/java/common/A.java"),
            &policy,
            &context,
        );
        assert!(!escaping.is_allowed());
        assert!(escaping.source.is_allowed());
        assert!(!escaping.destination.is_allowed());
        assert!(escaping.into_result().is_err());
    }

    #[test]
    fn a_move_into_the_allowed_scope_from_outside_is_refused() {
        let policy = compiled(POLICY);
        let decision = evaluate_move(
            &path("src/main/java/common/A.java"),
            &path("src/main/java/auth/A.java"),
            &policy,
            &context(&[], TaskId::new()),
        );
        assert!(!decision.is_allowed());
        assert!(!decision.source.is_allowed());
    }
}
