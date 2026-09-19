//! The desugared policy: what gets approved, stored and evaluated.
//!
//! [`NormalizedPolicy`] is plain data. It is the form written to the approved
//! policy store, so it serialises and carries no compiled state. [`CompiledPolicy`]
//! is that same policy with its patterns compiled, which is what evaluation uses.
//!
//! Desugaring happens here so evaluation only ever sees one shape: the shorthand
//! `allow = [...]` list and the long `[[scope.allow_rule]]` form both become the
//! same kind of entry, ordered by where they were written.

use serde::{Deserialize, Serialize};

use crate::domain::{OpSet, Operation};
use crate::error::Denial;

use super::defaults::SCHEMA_VERSION;
use super::file::{ConflictAction, ExpansionPolicy, PolicyDocument};
use super::matcher::{CaseSensitivity, Pattern, PatternSet};
use super::protected::ProtectedPaths;

/// One allow rule after desugaring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowEntry {
    pub pattern: String,
    pub ops: OpSet,
    /// Line in the source policy file, kept so a denial can point at the rule.
    pub line: Option<u32>,
}

/// One deny rule.
///
/// It carries no operation set: a deny is total. Encoding that in the type keeps
/// anyone from later adding a partial deny, which would make refusal conditional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenyEntry {
    pub pattern: String,
    pub line: Option<u32>,
}

/// How much change a task may make.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BudgetLimits {
    pub max_changed_paths: u64,
    pub max_moves: u64,
    pub max_operations: u64,
    pub max_file_bytes: u64,
    pub max_snapshot_bytes: u64,
    pub warn_at_ratio: f64,
}

/// What may be asked for at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalSettings {
    pub scope_expansion: ExpansionPolicy,
    pub budget_expansion: ExpansionPolicy,
    pub max_elicitations_per_task: u32,
    pub grant_ttl_minutes: u64,
}

/// Limits on what the file engine will attempt at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetySettings {
    pub follow_symlinks: bool,
    pub allow_cross_filesystem_move: bool,
    pub overwrite_move_destination: bool,
    pub create_parent_directories: bool,
    pub workspace_writer_limit: u32,
    pub unsafe_allow_workspace_wide: bool,
}

/// What happens when undo meets a later change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoverySettings {
    pub conflict_action: ConflictAction,
    pub retain_closed_task_days: u32,
    pub protect_incomplete_tasks: bool,
}

/// A policy in the form that is approved and stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedPolicy {
    pub schema_version: u32,
    pub allow: Vec<AllowEntry>,
    pub deny: Vec<DenyEntry>,
    pub budget: BudgetLimits,
    pub approval: ApprovalSettings,
    pub safety: SafetySettings,
    pub recovery: RecoverySettings,
}

impl NormalizedPolicy {
    /// Desugars a parsed policy file.
    ///
    /// This performs no validation beyond what parsing already did; see
    /// [`super::validate`], which runs against this form.
    pub fn from_document(document: &PolicyDocument) -> Self {
        let file = document.file();
        let scope = &file.scope;

        // An omitted default_ops means every operation. An explicitly empty one
        // stays empty so validation can reject it rather than silently widening
        // it into the opposite of what was written.
        let default_ops = scope
            .default_ops
            .as_ref()
            .map_or(OpSet::all(), |ops| ops.iter().copied().collect());

        let mut allow: Vec<AllowEntry> = scope
            .allow
            .iter()
            .map(|pattern| AllowEntry {
                pattern: pattern.get_ref().clone(),
                ops: default_ops,
                line: Some(document.line_of(pattern.span())),
            })
            .chain(scope.allow_rule.iter().map(|rule| {
                AllowEntry {
                    pattern: rule.path.get_ref().clone(),
                    ops: rule
                        .ops
                        .as_ref()
                        .map_or(default_ops, |ops| ops.iter().copied().collect()),
                    line: Some(document.line_of(rule.path.span())),
                }
            }))
            .collect();

        // Rules apply in the order they are written, which is not the order the
        // two TOML forms arrive in.
        allow.sort_by_key(|entry| entry.line);

        let deny = scope
            .deny
            .iter()
            .map(|pattern| DenyEntry {
                pattern: pattern.get_ref().clone(),
                line: Some(document.line_of(pattern.span())),
            })
            .collect();

        Self {
            schema_version: file.schema_version,
            allow,
            deny,
            budget: BudgetLimits {
                max_changed_paths: file.budget.max_changed_paths,
                max_moves: file.budget.max_moves,
                max_operations: file.budget.max_operations,
                max_file_bytes: file.budget.max_file_bytes,
                max_snapshot_bytes: file.budget.max_snapshot_bytes,
                warn_at_ratio: file.budget.warn_at_ratio,
            },
            approval: ApprovalSettings {
                scope_expansion: file.approval.scope_expansion,
                budget_expansion: file.approval.budget_expansion,
                max_elicitations_per_task: file.approval.max_elicitations_per_task,
                grant_ttl_minutes: file.approval.grant_ttl_minutes,
            },
            safety: SafetySettings {
                follow_symlinks: file.safety.follow_symlinks,
                allow_cross_filesystem_move: file.safety.allow_cross_filesystem_move,
                overwrite_move_destination: file.safety.overwrite_move_destination,
                create_parent_directories: file.safety.create_parent_directories,
                workspace_writer_limit: file.safety.workspace_writer_limit,
                unsafe_allow_workspace_wide: file.safety.unsafe_allow_workspace_wide,
            },
            recovery: RecoverySettings {
                conflict_action: file.recovery.conflict_action,
                retain_closed_task_days: file.recovery.retain_closed_task_days,
                protect_incomplete_tasks: file.recovery.protect_incomplete_tasks,
            },
        }
    }

    /// Convenience for tests and for `safescope init`: parse and desugar in one step.
    pub fn from_text(text: &str) -> Result<Self, Denial> {
        PolicyDocument::parse(text).map(|document| Self::from_document(&document))
    }
}

/// An allow rule with its pattern compiled.
#[derive(Debug, Clone)]
pub struct CompiledAllow {
    pub pattern: Pattern,
    pub ops: OpSet,
}

/// A normalised policy with its patterns compiled, ready to evaluate against.
#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    normalized: NormalizedPolicy,
    allow: Vec<CompiledAllow>,
    deny: PatternSet,
    protected: ProtectedPaths,
}

impl CompiledPolicy {
    /// Compiles every pattern, failing on the first unusable one.
    pub fn compile(normalized: NormalizedPolicy) -> Result<Self, Denial> {
        let allow = normalized
            .allow
            .iter()
            .map(|entry| {
                // Permit narrowly: an allow rule matches exactly.
                Pattern::compile(&entry.pattern, CaseSensitivity::Exact, entry.line).map(
                    |pattern| CompiledAllow {
                        pattern,
                        ops: entry.ops,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Refuse broadly: a deny rule also matches ignoring case.
        let deny = PatternSet::compile(
            normalized
                .deny
                .iter()
                .map(|entry| (entry.pattern.as_str(), entry.line)),
            CaseSensitivity::AlsoIgnoringCase,
        )?;

        Ok(Self {
            normalized,
            allow,
            deny,
            protected: ProtectedPaths::engine_defaults(),
        })
    }

    pub const fn normalized(&self) -> &NormalizedPolicy {
        &self.normalized
    }

    pub fn allow(&self) -> &[CompiledAllow] {
        &self.allow
    }

    pub const fn deny(&self) -> &PatternSet {
        &self.deny
    }

    pub const fn protected(&self) -> &ProtectedPaths {
        &self.protected
    }

    pub const fn budget(&self) -> &BudgetLimits {
        &self.normalized.budget
    }

    pub const fn approval(&self) -> &ApprovalSettings {
        &self.normalized.approval
    }

    pub const fn safety(&self) -> &SafetySettings {
        &self.normalized.safety
    }

    pub const fn recovery(&self) -> &RecoverySettings {
        &self.normalized.recovery
    }
}

/// The policy `safescope init` writes: standard refusals, no permissions.
///
/// An empty allow list means every write is refused until a person fills it in
/// and approves. Shipping a default that permits something would mean nobody
/// reads the policy.
pub fn starter_policy_text() -> String {
    let operations = Operation::ALL
        .iter()
        .map(|operation| format!("\"{operation}\""))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "schema_version = {SCHEMA_VERSION}

# Nothing can be changed until this list is filled in and approved with
#   safescope policy approve
[scope]
allow = [
]

deny = [
  \"**/.env\",
  \"**/.env.*\",
  \"**/*.pem\",
  \"**/secrets/**\",
]

default_ops = [{operations}]
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorthand_rules_inherit_the_default_operations() {
        let policy = NormalizedPolicy::from_text(
            "schema_version = 1\n\
             [scope]\n\
             allow = [\"src/**\"]\n\
             default_ops = [\"create\", \"replace\"]\n",
        )
        .unwrap();

        assert_eq!(policy.allow.len(), 1);
        assert!(policy.allow[0].ops.contains(Operation::Create));
        assert!(policy.allow[0].ops.contains(Operation::Replace));
        assert!(!policy.allow[0].ops.contains(Operation::Trash));
    }

    #[test]
    fn an_absent_default_operations_list_means_all_of_them() {
        let policy =
            NormalizedPolicy::from_text("schema_version = 1\n[scope]\nallow = [\"src/**\"]\n")
                .unwrap();
        assert_eq!(policy.allow[0].ops, OpSet::all());
    }

    #[test]
    fn long_form_rules_override_the_default_operations() {
        let policy = NormalizedPolicy::from_text(
            "schema_version = 1\n\
             [scope]\n\
             default_ops = [\"create\", \"replace\", \"move\", \"trash\"]\n\
             \n\
             [[scope.allow_rule]]\n\
             path = \"src/resources/**\"\n\
             ops = [\"replace\"]\n",
        )
        .unwrap();

        let entry = &policy.allow[0];
        assert_eq!(entry.pattern, "src/resources/**");
        assert!(entry.ops.contains(Operation::Replace));
        assert!(!entry.ops.contains(Operation::Trash));
    }

    #[test]
    fn long_form_rules_without_operations_inherit_the_default() {
        let policy = NormalizedPolicy::from_text(
            "schema_version = 1\n\
             [scope]\n\
             default_ops = [\"replace\"]\n\
             \n\
             [[scope.allow_rule]]\n\
             path = \"src/**\"\n",
        )
        .unwrap();
        assert_eq!(
            policy.allow[0].ops,
            [Operation::Replace].into_iter().collect()
        );
    }

    #[test]
    fn rules_are_ordered_by_where_they_are_written() {
        // The long form is declared first in the file but arrives second from
        // the parser; declaration order is what a reader expects to apply.
        let policy = NormalizedPolicy::from_text(
            "schema_version = 1\n\
             \n\
             [[scope.allow_rule]]\n\
             path = \"first/**\"\n\
             \n\
             [scope]\n\
             allow = [\"second/**\"]\n",
        )
        .unwrap();

        let patterns: Vec<_> = policy
            .allow
            .iter()
            .map(|entry| entry.pattern.as_str())
            .collect();
        assert_eq!(patterns, ["first/**", "second/**"]);
    }

    #[test]
    fn deny_rules_keep_their_line() {
        let policy = NormalizedPolicy::from_text(
            "schema_version = 1\n\
             [scope]\n\
             allow = [\"src/**\"]\n\
             deny = [\"**/.env\"]\n",
        )
        .unwrap();
        assert_eq!(policy.deny.len(), 1);
        assert_eq!(policy.deny[0].line, Some(4));
    }

    #[test]
    fn the_stored_form_round_trips_through_json() {
        let policy = NormalizedPolicy::from_text(
            "schema_version = 1\n\
             [scope]\n\
             allow = [\"src/**\"]\n\
             deny = [\"**/.env\"]\n",
        )
        .unwrap();

        let json = serde_json::to_string(&policy).unwrap();
        let restored: NormalizedPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, policy);
    }

    #[test]
    fn compiling_rejects_an_unusable_pattern() {
        let policy = NormalizedPolicy::from_text(
            "schema_version = 1\n[scope]\nallow = [\"../outside/**\"]\n",
        )
        .unwrap();
        assert!(CompiledPolicy::compile(policy).is_err());
    }

    #[test]
    fn a_compiled_policy_exposes_its_settings() {
        let policy = NormalizedPolicy::from_text(
            "schema_version = 1\n\
             [scope]\n\
             allow = [\"src/**\"]\n\
             deny = [\"**/.env\"]\n",
        )
        .unwrap();
        let compiled = CompiledPolicy::compile(policy).unwrap();

        assert_eq!(compiled.allow().len(), 1);
        assert_eq!(compiled.deny().len(), 1);
        assert_eq!(compiled.budget().max_changed_paths, 8);
        assert!(!compiled.safety().follow_symlinks);
        assert!(
            compiled
                .protected()
                .matches(&crate::paths::RelPath::parse(".git/x").unwrap())
        );
    }

    #[test]
    fn the_starter_policy_parses_but_permits_nothing() {
        let policy = NormalizedPolicy::from_text(&starter_policy_text()).unwrap();
        assert!(
            policy.allow.is_empty(),
            "a starter policy must grant nothing"
        );
        assert!(
            !policy.deny.is_empty(),
            "it should still carry the standard refusals"
        );
        assert!(CompiledPolicy::compile(policy).is_ok());
    }
}
