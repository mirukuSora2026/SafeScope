//! Policy validation, run when a policy is approved.
//!
//! A policy that is wrong should fail loudly at approval time rather than
//! silently doing nothing later. The case that matters most is an `allow` rule
//! over a protected path: ignoring it quietly leaves the author believing they
//! granted something they did not.
//!
//! Errors block approval. Warnings do not — they describe rules that have no
//! effect, which is worth saying but is not a reason to refuse.

use crate::dataformatting::Msg;
use crate::domain::OpSet;
use crate::error::{Denial, ErrorCode};

use super::file::PolicyDocument;
use super::matcher::{CaseSensitivity, Pattern};
use super::normalized::NormalizedPolicy;
use super::protected::{ProtectedPaths, patterns_overlap};

/// Whether a finding blocks approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Approval is refused.
    Error,
    /// Approval proceeds, but the user is told.
    Warning,
}

/// One validation finding.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// Line in the policy file, when the finding belongs to a specific rule.
    pub line: Option<u32>,
}

impl Diagnostic {
    fn error(message: Msg, line: Option<u32>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.to_string(),
            line,
        }
    }

    fn warning(message: Msg, line: Option<u32>) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.to_string(),
            line,
        }
    }
}

/// Everything validation found.
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.of(Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.of(Severity::Warning)
    }

    fn of(&self, severity: Severity) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(move |entry| entry.severity == severity)
    }

    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }

    /// The denial to raise when approval cannot proceed.
    ///
    /// Every error is included: fixing one at a time through repeated approval
    /// attempts is a poor way to spend a person's afternoon.
    pub fn to_denial(&self) -> Option<Denial> {
        let errors: Vec<String> = self
            .errors()
            .map(|entry| match entry.line {
                Some(line) => format!("policy.toml:{line}: {}", entry.message),
                None => entry.message.clone(),
            })
            .collect();
        if errors.is_empty() {
            return None;
        }
        Some(Denial::new(ErrorCode::PolicyInvalid, errors.join("\n")))
    }

    fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }
}

/// Validates a desugared policy.
///
/// Takes the document as well, because two checks need what desugaring
/// deliberately discards: whether `default_ops` was written as an empty list,
/// and the exact source text.
pub fn validate(
    document: &PolicyDocument,
    policy: &NormalizedPolicy,
    protected: &ProtectedPaths,
) -> ValidationReport {
    let mut report = ValidationReport::default();

    check_scope_is_usable(document, policy, &mut report);
    check_allow_rules(policy, protected, &mut report);
    check_allow_against_deny(policy, &mut report);
    check_duplicates(policy, &mut report);
    check_deny_reaches_something(policy, &mut report);
    check_budget(policy, &mut report);

    report
}

fn check_scope_is_usable(
    document: &PolicyDocument,
    policy: &NormalizedPolicy,
    report: &mut ValidationReport,
) {
    if policy.allow.is_empty() {
        // A policy that can never permit anything is far more often a mistake
        // than a deliberate lockout.
        report.push(Diagnostic::error(Msg::PolicyNoAllowRules, None));
    }
    if document
        .file()
        .scope
        .default_ops
        .as_ref()
        .is_some_and(|ops| ops.is_empty())
    {
        report.push(Diagnostic::error(Msg::PolicyEmptyDefaultOps, None));
    }
}

fn check_allow_rules(
    policy: &NormalizedPolicy,
    protected: &ProtectedPaths,
    report: &mut ValidationReport,
) {
    for entry in &policy.allow {
        if entry.ops == OpSet::empty() {
            report.push(Diagnostic::error(
                Msg::PolicyRuleGrantsNothing {
                    pattern: entry.pattern.clone(),
                },
                entry.line,
            ));
        }

        if is_workspace_wide(&entry.pattern) {
            if !policy.safety.unsafe_allow_workspace_wide {
                report.push(Diagnostic::error(
                    Msg::PolicyWorkspaceWideNeedsOptIn {
                        pattern: entry.pattern.clone(),
                    },
                    entry.line,
                ));
            }
            // The protected-path check below looks for a rule *aimed* at
            // something protected, where the author believes they granted access
            // they did not get. A blanket rule is not that: protected paths
            // simply carve out of it, and the author said so explicitly.
            // Reporting a conflict here would make the opt-in impossible to use.
            continue;
        }

        // Compilation failures are reported by CompiledPolicy::compile, which
        // runs separately; a pattern that will not compile is skipped here
        // rather than reported twice.
        let Ok(pattern) = Pattern::compile(&entry.pattern, CaseSensitivity::Exact, entry.line)
        else {
            continue;
        };
        if let Some(conflict) = protected.conflicting_pattern(&pattern) {
            report.push(Diagnostic::error(
                Msg::PolicyAllowOverProtected {
                    pattern: entry.pattern.clone(),
                    protected: conflict.text().to_owned(),
                },
                entry.line,
            ));
        }
    }
}

fn check_allow_against_deny(policy: &NormalizedPolicy, report: &mut ValidationReport) {
    for entry in &policy.allow {
        if policy.deny.iter().any(|deny| deny.pattern == entry.pattern) {
            report.push(Diagnostic::error(
                Msg::PolicyAllowAlsoDenied {
                    pattern: entry.pattern.clone(),
                },
                entry.line,
            ));
        }
    }
}

fn check_duplicates(policy: &NormalizedPolicy, report: &mut ValidationReport) {
    let mut seen: Vec<&str> = Vec::new();
    for (pattern, line) in policy
        .allow
        .iter()
        .map(|entry| (entry.pattern.as_str(), entry.line))
        .chain(
            policy
                .deny
                .iter()
                .map(|entry| (entry.pattern.as_str(), entry.line)),
        )
    {
        if seen.contains(&pattern) {
            report.push(Diagnostic::warning(
                Msg::PolicyDuplicatePattern {
                    pattern: pattern.to_owned(),
                },
                line,
            ));
        } else {
            seen.push(pattern);
        }
    }
}

fn check_deny_reaches_something(policy: &NormalizedPolicy, report: &mut ValidationReport) {
    for deny in &policy.deny {
        let reaches = policy
            .allow
            .iter()
            .any(|allow| patterns_overlap(&deny.pattern, &allow.pattern));
        if !reaches {
            report.push(Diagnostic::warning(
                Msg::PolicyDenyNeverApplies {
                    pattern: deny.pattern.clone(),
                },
                deny.line,
            ));
        }
    }
}

fn check_budget(policy: &NormalizedPolicy, report: &mut ValidationReport) {
    let budget = &policy.budget;
    let zero_fields = [
        ("max_changed_paths", budget.max_changed_paths),
        ("max_operations", budget.max_operations),
        ("max_file_bytes", budget.max_file_bytes),
        ("max_snapshot_bytes", budget.max_snapshot_bytes),
    ];
    for (name, value) in zero_fields {
        if value == 0 {
            report.push(Diagnostic::error(
                Msg::PolicyBudgetZero {
                    field: name.to_owned(),
                },
                None,
            ));
        }
    }

    if budget.max_file_bytes > budget.max_snapshot_bytes {
        // A file large enough to permit but too large to snapshot could never be
        // changed safely, so the pair is contradictory.
        report.push(Diagnostic::error(
            Msg::PolicyFileLimitExceedsSnapshotLimit {
                file_bytes: budget.max_file_bytes,
                snapshot_bytes: budget.max_snapshot_bytes,
            },
            None,
        ));
    }

    if !(budget.warn_at_ratio > 0.0 && budget.warn_at_ratio <= 1.0) {
        report.push(Diagnostic::error(
            Msg::PolicyWarnRatioOutOfRange {
                value: budget.warn_at_ratio,
            },
            None,
        ));
    }
}

/// Whether a pattern reaches every file in the workspace.
fn is_workspace_wide(pattern: &str) -> bool {
    matches!(pattern.trim(), "**" | "**/*" | "**/**")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let report = report_for(&super::super::normalized::starter_policy_text());
        assert!(report.has_errors());
    }
}
