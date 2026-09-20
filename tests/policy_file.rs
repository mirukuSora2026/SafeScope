//! The policy file schema: what a written policy parses into.
//!
//! These moved out of `src/policy/file.rs` when it outgrew the file-length
//! limit. They read better here anyway: every one of them goes through
//! `PolicyDocument::parse`, which is the public way in.

use safescope::domain::Operation;
use safescope::error::ErrorCode;
use safescope::policy::PolicyDocument;
use safescope::policy::defaults::{
    DEFAULT_MAX_CHANGED_PATHS, DEFAULT_MAX_MOVES, DEFAULT_MAX_OPERATIONS,
};
use safescope::policy::file::{ConflictAction, EnforcementMode, ExpansionPolicy};

const MINIMAL: &str = "schema_version = 1\n";

#[test]
fn parses_a_minimal_file_with_defaults() {
    let document = PolicyDocument::parse(MINIMAL).unwrap();
    let file = document.file();
    assert!(file.scope.allow.is_empty());
    assert_eq!(file.budget.max_changed_paths, DEFAULT_MAX_CHANGED_PATHS);
    assert_eq!(file.budget.max_moves, DEFAULT_MAX_MOVES);
    assert_eq!(file.approval.scope_expansion, ExpansionPolicy::Request);
    assert!(!file.safety.follow_symlinks);
    assert!(!file.safety.create_parent_directories);
    assert_eq!(file.recovery.conflict_action, ConflictAction::Stop);
    assert!(file.recovery.protect_incomplete_tasks);
}

#[test]
fn rejects_a_missing_schema_version() {
    assert!(PolicyDocument::parse("[scope]\nallow = []\n").is_err());
}

#[test]
fn rejects_an_unsupported_schema_version() {
    let denial = PolicyDocument::parse("schema_version = 99\n").unwrap_err();
    assert_eq!(denial.code(), ErrorCode::PolicyInvalid);
}

#[test]
fn rejects_an_unknown_key() {
    // A misspelled `deney` that parsed as "no deny rules" would hand out
    // permissions the author believed they had withheld.
    let text = "schema_version = 1\n[scope]\ndeney = [\"**/.env\"]\n";
    assert!(PolicyDocument::parse(text).is_err());
}

#[test]
fn rejects_an_unknown_top_level_section() {
    let text = "schema_version = 1\n[scopes]\nallow = []\n";
    assert!(PolicyDocument::parse(text).is_err());
}

#[test]
fn reads_the_shorthand_scope_form() {
    let text = "\
schema_version = 1

[scope]
allow = [
  \"src/main/java/auth/**\",
  \"src/test/java/auth/**\",
]
deny = [\"**/.env\"]
default_ops = [\"create\", \"replace\"]
";
    let document = PolicyDocument::parse(text).unwrap();
    let scope = &document.file().scope;
    assert_eq!(scope.allow.len(), 2);
    assert_eq!(scope.deny.len(), 1);
    assert_eq!(
        scope.default_ops.as_deref(),
        Some(&[Operation::Create, Operation::Replace][..])
    );
}

#[test]
fn reads_the_long_scope_form() {
    let text = "\
schema_version = 1

[[scope.allow_rule]]
path = \"src/main/resources/**\"
ops = [\"replace\"]

[[scope.allow_rule]]
path = \"src/test/**\"
";
    let document = PolicyDocument::parse(text).unwrap();
    let rules = &document.file().scope.allow_rule;
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].path.get_ref(), "src/main/resources/**");
    assert_eq!(rules[0].ops.as_deref(), Some(&[Operation::Replace][..]));
    assert!(
        rules[1].ops.is_none(),
        "an absent ops list inherits the default"
    );
}

#[test]
fn reports_the_line_a_rule_is_written_on() {
    let text = "\
schema_version = 1

[scope]
allow = [
  \"src/auth/**\",
  \"src/test/**\",
]
";
    let document = PolicyDocument::parse(text).unwrap();
    let allow = &document.file().scope.allow;
    assert_eq!(document.line_of(allow[0].span()), 5);
    assert_eq!(document.line_of(allow[1].span()), 6);
}

#[test]
fn reports_lines_for_long_form_rules() {
    let text = "\
schema_version = 1

[[scope.allow_rule]]
path = \"src/a/**\"
ops = [\"replace\"]

[[scope.allow_rule]]
path = \"src/b/**\"
";
    let document = PolicyDocument::parse(text).unwrap();
    let rules = &document.file().scope.allow_rule;
    assert_eq!(document.line_of(rules[0].path.span()), 4);
    assert_eq!(document.line_of(rules[1].path.span()), 8);
}

#[test]
fn reads_every_section() {
    let text = "\
schema_version = 1

[budget]
max_changed_paths = 12
max_file_bytes = 1024

[approval]
scope_expansion = \"never\"
grant_ttl_minutes = 5

[safety]
unsafe_allow_workspace_wide = true
workspace_writer_limit = 2

[recovery]
retain_closed_task_days = 30
protect_incomplete_tasks = false
";
    let document = PolicyDocument::parse(text).unwrap();
    let file = document.file();
    assert_eq!(file.budget.max_changed_paths, 12);
    assert_eq!(file.budget.max_file_bytes, 1024);
    // Untouched fields keep their defaults.
    assert_eq!(file.budget.max_operations, DEFAULT_MAX_OPERATIONS);
    assert_eq!(file.approval.scope_expansion, ExpansionPolicy::Never);
    assert_eq!(file.approval.budget_expansion, ExpansionPolicy::Request);
    assert_eq!(file.approval.grant_ttl_minutes, 5);
    assert!(file.safety.unsafe_allow_workspace_wide);
    assert_eq!(file.safety.workspace_writer_limit, 2);
    assert_eq!(file.recovery.retain_closed_task_days, 30);
    assert!(!file.recovery.protect_incomplete_tasks);
}

#[test]
fn rejects_an_unknown_operation_name() {
    let text = "schema_version = 1\n[scope]\ndefault_ops = [\"delete\"]\n";
    assert!(PolicyDocument::parse(text).is_err());
}

#[test]
fn enforcement_defaults_to_judging_only_what_it_can_read() {
    let document = PolicyDocument::parse("schema_version = 1\n").unwrap();
    let enforcement = &document.file().enforcement;
    assert_eq!(enforcement.mode, EnforcementMode::Audit);
    assert!(enforcement.allow_tools.is_empty());
}

#[test]
fn an_allowlist_has_to_be_asked_for_by_name() {
    let document = PolicyDocument::parse(
        "schema_version = 1\n\
         [enforcement]\n\
         mode = \"allowlist\"\n\
         allow_tools = [\"WebSearch\"]\n",
    )
    .unwrap();
    let enforcement = &document.file().enforcement;
    assert_eq!(enforcement.mode, EnforcementMode::Allowlist);
    assert_eq!(enforcement.allow_tools[0].get_ref(), "WebSearch");
}

#[test]
fn an_unknown_enforcement_mode_is_refused_rather_than_assumed() {
    // Silently falling back to audit would turn a typo into a workspace that
    // reports itself as locked down and is not.
    let denial = PolicyDocument::parse(
        "schema_version = 1\n\
         [enforcement]\n\
         mode = \"allowlst\"\n",
    )
    .unwrap_err();
    assert_eq!(denial.code(), ErrorCode::PolicyInvalid);
}
