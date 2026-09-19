//! Scope evaluation, exercised through the crate's public surface.
//!
//! Lives outside the crate because this is the behaviour a user of SafeScope
//! cares about — "why was this path refused?" — and because evaluate.rs is
//! large enough without it.

use std::time::{Duration, SystemTime};

use safescope::domain::{OpSet, Operation};
use safescope::error::ErrorCode;
use safescope::ids::{PlanId, TaskId};
use safescope::paths::RelPath;
use safescope::policy::{
    ApprovalSource, Authority, CompiledPolicy, Decision, EvaluationContext, Grant,
    NormalizedPolicy, PolicyVersion, RuleSource, evaluate, evaluate_move,
};

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
        authority: Authority::Requested,
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
