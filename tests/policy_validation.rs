//! What approval refuses and what it warns about.
//!
//! These moved out of `src/policy/validate.rs` when it outgrew the file-length
//! limit. Every one goes in through `PolicyDocument::parse`, which is how a
//! policy actually arrives.

use safescope::error::ErrorCode;
use safescope::policy::{
    NormalizedPolicy, PolicyDocument, ProtectedPaths, ValidationReport, validate,
};

fn report_for(text: &str) -> ValidationReport {
    let document = PolicyDocument::parse(text).expect("parses");
    let policy = NormalizedPolicy::from_document(&document);
    validate(&document, &policy, &ProtectedPaths::engine_defaults())
}

fn error_messages(report: &ValidationReport) -> Vec<&str> {
    report
        .errors()
        .map(|entry| entry.message.as_str())
        .collect()
}

const USABLE: &str = "schema_version = 1\n[scope]\nallow = [\"src/**\"]\n";

#[test]
fn a_usable_policy_has_no_errors() {
    let report = report_for(USABLE);
    assert!(!report.has_errors(), "{:?}", error_messages(&report));
    assert!(report.to_denial().is_none());
}

#[test]
fn rejects_a_policy_with_no_allow_rules() {
    let report = report_for("schema_version = 1\n[scope]\ndeny = [\"**/.env\"]\n");
    assert!(report.has_errors());
    assert!(report.to_denial().is_some());
}

#[test]
fn rejects_an_explicitly_empty_default_operations_list() {
    let report =
        report_for("schema_version = 1\n[scope]\nallow = [\"src/**\"]\ndefault_ops = []\n");
    assert!(report.has_errors());
    // Both the empty list and the rule it renders useless are reported.
    assert_eq!(report.errors().count(), 2);
}

#[test]
fn rejects_a_rule_that_grants_nothing() {
    let report = report_for(
        "schema_version = 1\n\
         \n\
         [[scope.allow_rule]]\n\
         path = \"src/**\"\n\
         ops = []\n",
    );
    assert!(report.has_errors());
    let denial = report.to_denial().expect("denial");
    assert_eq!(denial.code(), ErrorCode::PolicyInvalid);
    assert!(
        denial.message().contains("policy.toml:4"),
        "{}",
        denial.message()
    );
}

#[test]
fn rejects_an_allow_rule_over_a_protected_path() {
    // The check that matters most: silently ignoring this would leave the
    // author believing they granted something they did not.
    let report = report_for("schema_version = 1\n[scope]\nallow = [\".git/**\"]\n");
    assert!(report.has_errors());
}

#[test]
fn rejects_a_workspace_wide_rule_without_the_opt_in() {
    let report = report_for("schema_version = 1\n[scope]\nallow = [\"**\"]\n");
    assert!(report.has_errors());
    assert_eq!(
        report.errors().count(),
        1,
        "one finding, not one per protected path"
    );
}

#[test]
fn accepts_a_workspace_wide_rule_with_the_opt_in() {
    let report = report_for(
        "schema_version = 1\n\
         [scope]\n\
         allow = [\"**\"]\n\
         \n\
         [safety]\n\
         unsafe_allow_workspace_wide = true\n",
    );
    assert!(!report.has_errors(), "{:?}", error_messages(&report));
}

#[test]
fn rejects_a_pattern_that_is_both_allowed_and_denied() {
    let report =
        report_for("schema_version = 1\n[scope]\nallow = [\"src/**\"]\ndeny = [\"src/**\"]\n");
    assert!(report.has_errors());
}

#[test]
fn rejects_a_zero_budget() {
    let report = report_for(&format!("{USABLE}\n[budget]\nmax_operations = 0\n"));
    assert!(report.has_errors());
}

#[test]
fn rejects_a_file_limit_larger_than_the_snapshot_limit() {
    let report = report_for(&format!(
        "{USABLE}\n[budget]\nmax_file_bytes = 2048\nmax_snapshot_bytes = 1024\n"
    ));
    assert!(report.has_errors());
}

#[test]
fn rejects_a_warning_ratio_outside_its_range() {
    for ratio in ["0.0", "1.5", "-0.2"] {
        let report = report_for(&format!("{USABLE}\n[budget]\nwarn_at_ratio = {ratio}\n"));
        assert!(
            report.has_errors(),
            "warn_at_ratio = {ratio} should be rejected"
        );
    }
}

#[test]
fn warns_about_a_duplicate_pattern_without_blocking() {
    let report = report_for("schema_version = 1\n[scope]\nallow = [\"src/**\", \"src/**\"]\n");
    assert!(!report.has_errors());
    assert_eq!(report.warnings().count(), 1);
}

#[test]
fn warns_about_a_deny_rule_that_reaches_nothing() {
    let report =
        report_for("schema_version = 1\n[scope]\nallow = [\"src/**\"]\ndeny = [\"docs/**\"]\n");
    assert!(!report.has_errors());
    assert_eq!(report.warnings().count(), 1);
}

#[test]
fn a_deny_rule_that_reaches_an_allowed_path_is_not_warned_about() {
    let report = report_for(
        "schema_version = 1\n[scope]\nallow = [\"src/**\"]\ndeny = [\"src/generated/**\"]\n",
    );
    assert_eq!(report.warnings().count(), 0);
}

#[test]
fn every_error_is_reported_at_once() {
    // Fixing one finding per approval attempt is a poor use of an afternoon.
    let report = report_for(
        "schema_version = 1\n\
         [scope]\n\
         allow = [\".git/**\"]\n\
         \n\
         [budget]\n\
         max_operations = 0\n\
         warn_at_ratio = 3.0\n",
    );
    assert!(
        report.errors().count() >= 3,
        "{:?}",
        error_messages(&report)
    );
    let denial = report.to_denial().expect("denial");
    assert_eq!(denial.message().lines().count(), report.errors().count());
}

#[test]
fn the_starter_policy_is_rejected_until_it_is_filled_in() {
    // It grants nothing on purpose, and validation says so rather than
    // approving a policy that can never permit anything.
    let report = report_for(&safescope::policy::starter_policy_text());
    assert!(report.has_errors());
}

/// Warnings about the enforcement section.
///
/// Both are a person's call to make, and both are wrong to discover later —
/// which is what happens when approval prints nothing about them.
mod enforcement {
    use super::report_for;

    fn warnings(text: &str) -> Vec<String> {
        report_for(text)
            .warnings()
            .map(|entry| entry.message.clone())
            .collect()
    }

    const ALLOWLIST: &str = "schema_version = 1\n\
                             [scope]\n\
                             allow = [\"src/**\"]\n\
                             [enforcement]\n\
                             mode = \"allowlist\"\n";

    #[test]
    fn allowing_a_tool_that_runs_commands_is_warned_about() {
        // Naming Bash here gives back the route the mode exists to close, and
        // the workspace then reports itself as locked down while it is not.
        let warnings = warnings(&format!("{ALLOWLIST}allow_tools = [\"Bash\"]\n"));
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("Bash") && w.contains("shell command")),
            "nothing warned about Bash: {warnings:?}"
        );
    }

    #[test]
    fn every_measured_escape_route_is_warned_about() {
        // These are the four an agent actually reached for, plus Task, which is
        // the same shape. A list that warned about only the famous one would
        // leave the others looking approved.
        for tool in ["Bash", "Monitor", "Agent", "Task", "Skill"] {
            let warnings = warnings(&format!("{ALLOWLIST}allow_tools = [\"{tool}\"]\n"));
            assert!(
                warnings.iter().any(|w| w.contains(tool)),
                "{tool} was allowed without a word"
            );
        }
    }

    #[test]
    fn an_ordinary_tool_is_not_warned_about() {
        // The warning has to mean something, so it cannot fire on everything.
        let warnings = warnings(&format!("{ALLOWLIST}allow_tools = [\"WebSearch\"]\n"));
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn a_list_that_cannot_do_anything_is_warned_about() {
        // `allow_tools` in audit mode is configuration that looks like it works.
        let warnings = warnings(
            "schema_version = 1\n\
             [scope]\n\
             allow = [\"src/**\"]\n\
             [enforcement]\n\
             allow_tools = [\"WebSearch\"]\n",
        );
        assert!(
            warnings.iter().any(|w| w.contains("no effect")),
            "an inert list passed without comment: {warnings:?}"
        );
    }

    #[test]
    fn an_allowlist_with_no_extra_tools_is_quiet() {
        assert!(warnings(ALLOWLIST).is_empty());
    }
}
