//! The on-disk policy file: its schema, and reading it with line numbers.
//!
//! This is the shape a person edits. It is never what the engine evaluates
//! against — [`super::normalized`] holds that. Keeping the two apart is what
//! makes "editing policy.toml grants nothing until it is approved" true by
//! construction rather than by discipline.
//!
//! Every section sets `deny_unknown_fields`. A misspelled `deney = [...]` that
//! parsed as "no deny rules" would hand out permissions the author believed they
//! had withheld, so an unknown key is an error rather than a shrug.

use std::ops::Range;

use serde::{Deserialize, Serialize};
use toml::Spanned;

use crate::dataformatting::Msg;
use crate::domain::Operation;
use crate::error::{Denial, ErrorCode};

use super::defaults::{
    DEFAULT_GRANT_TTL_MINUTES, DEFAULT_MAX_CHANGED_PATHS, DEFAULT_MAX_ELICITATIONS_PER_TASK,
    DEFAULT_MAX_FILE_BYTES, DEFAULT_MAX_MOVES, DEFAULT_MAX_OPERATIONS, DEFAULT_MAX_SNAPSHOT_BYTES,
    DEFAULT_RETAIN_CLOSED_TASK_DAYS, DEFAULT_WARN_AT_RATIO, DEFAULT_WORKSPACE_WRITER_LIMIT,
    SCHEMA_VERSION, default_grant_ttl_minutes, default_max_changed_paths,
    default_max_elicitations_per_task, default_max_file_bytes, default_max_moves,
    default_max_operations, default_max_snapshot_bytes, default_retain_closed_task_days,
    default_true, default_warn_at_ratio, default_workspace_writer_limit,
};

/// A parsed policy file together with its source text, so spans can be turned
/// into line numbers.
#[derive(Debug)]
pub struct PolicyDocument {
    file: PolicyFile,
    lines: LineIndex,
}

impl PolicyDocument {
    /// Parses policy text, checking the schema version before anything else.
    pub fn parse(text: &str) -> Result<Self, Denial> {
        let file: PolicyFile = toml::from_str(text).map_err(|error| {
            Denial::new(
                ErrorCode::PolicyInvalid,
                Msg::PolicyParseFailed {
                    reason: error.to_string(),
                },
            )
        })?;
        if file.schema_version != SCHEMA_VERSION {
            return Err(Denial::new(
                ErrorCode::PolicyInvalid,
                Msg::PolicySchemaUnsupported {
                    found: file.schema_version,
                    supported: SCHEMA_VERSION,
                },
            ));
        }
        Ok(Self {
            file,
            lines: LineIndex::new(text),
        })
    }

    pub const fn file(&self) -> &PolicyFile {
        &self.file
    }

    /// The 1-based line a span starts on.
    pub fn line_of(&self, span: Range<usize>) -> u32 {
        self.lines.line_of(span.start)
    }
}

/// The policy file schema.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyFile {
    /// Required. An absent version is an error rather than an assumption.
    pub schema_version: u32,
    #[serde(default)]
    pub scope: ScopeSection,
    #[serde(default)]
    pub budget: BudgetSection,
    #[serde(default)]
    pub approval: ApprovalSection,
    #[serde(default)]
    pub safety: SafetySection,
    #[serde(default)]
    pub recovery: RecoverySection,
    #[serde(default)]
    pub enforcement: EnforcementSection,
}

/// What may be changed, and how.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeSection {
    /// Shorthand allow patterns, which inherit `default_ops`.
    #[serde(default)]
    pub allow: Vec<Spanned<String>>,
    /// Refusals. These carry no operation set: a deny is total.
    #[serde(default)]
    pub deny: Vec<Spanned<String>>,
    /// Operations the shorthand form grants.
    ///
    /// Absent means "every operation". An explicitly empty list is a different
    /// thing — a policy that grants nothing — and validation rejects it, so the
    /// two cases must stay distinguishable.
    #[serde(default)]
    pub default_ops: Option<Vec<Operation>>,
    /// Long form, for rules that need their own operation set.
    #[serde(default)]
    pub allow_rule: Vec<AllowRule>,
}

/// An allow rule with an explicit operation set.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllowRule {
    pub path: Spanned<String>,
    /// Absent means "inherit `default_ops`", which is not the same as an empty
    /// list — an empty list is a rule that permits nothing and is rejected.
    pub ops: Option<Vec<Operation>>,
}

/// How much change a task may make.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetSection {
    #[serde(default = "default_max_changed_paths")]
    pub max_changed_paths: u64,
    #[serde(default = "default_max_moves")]
    pub max_moves: u64,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
    #[serde(default = "default_max_file_bytes")]
    pub max_file_bytes: u64,
    #[serde(default = "default_max_snapshot_bytes")]
    pub max_snapshot_bytes: u64,
    /// Fraction of a limit at which the status output starts warning.
    #[serde(default = "default_warn_at_ratio")]
    pub warn_at_ratio: f64,
}

/// What may be asked for at runtime.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalSection {
    #[serde(default)]
    pub scope_expansion: ExpansionPolicy,
    #[serde(default)]
    pub budget_expansion: ExpansionPolicy,
    /// How many approvals a task may collect through the client before the next
    /// one has to be given at a terminal.
    #[serde(default = "default_max_elicitations_per_task")]
    pub max_elicitations_per_task: u32,
    #[serde(default = "default_grant_ttl_minutes")]
    pub grant_ttl_minutes: u64,
}

/// Whether an expansion may be requested at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExpansionPolicy {
    /// The engine may ask the user.
    #[default]
    Request,
    /// The engine refuses without asking.
    Never,
}

/// Limits on what the file engine will attempt at all.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetySection {
    #[serde(default)]
    pub follow_symlinks: bool,
    #[serde(default)]
    pub allow_cross_filesystem_move: bool,
    #[serde(default)]
    pub overwrite_move_destination: bool,
    /// v1 never creates directories: a new directory has no snapshot, no budget
    /// line and no defined undo, so three invariants would break at once.
    #[serde(default)]
    pub create_parent_directories: bool,
    #[serde(default = "default_workspace_writer_limit")]
    pub workspace_writer_limit: u32,
    /// Required before `**` may appear in an allow rule.
    #[serde(default)]
    pub unsafe_allow_workspace_wide: bool,
}

/// What the PreToolUse hook does about tools other than the edit tools.
///
/// This is the one part of the policy the engine cannot enforce, because the
/// engine never sees the tools it concerns. It is advice the host is asked to
/// follow, and a host that does not — a disabled hook, an older build — simply
/// does not. Drift detection is what notices afterwards.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnforcementSection {
    #[serde(default)]
    pub mode: EnforcementMode,
    /// Tools permitted in addition to the built-in read-only set.
    ///
    /// Named individually. A pattern here would be a way to write `*` by
    /// accident, which is the mode this section exists to leave behind.
    #[serde(default)]
    pub allow_tools: Vec<Spanned<String>>,
}

/// How much the hook refuses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementMode {
    /// Judge the tools that carry a path; say nothing about the rest.
    ///
    /// The default, because the alternative denies `Bash` outright and a great
    /// deal of ordinary work — running the tests, reading git log — is `Bash`.
    #[default]
    Audit,
    /// Refuse every tool not on the allowlist.
    ///
    /// `Bash` is not on it and cannot be judged onto it: a shell command is not
    /// reliably readable, so the choice is all of it or none. Opting in means
    /// choosing none, and accepting that the agent cannot run a command.
    Allowlist,
}

/// What happens when undo meets a later change.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverySection {
    #[serde(default)]
    pub conflict_action: ConflictAction,
    #[serde(default = "default_retain_closed_task_days")]
    pub retain_closed_task_days: u32,
    /// Keeps recovery data for unfinished work past the retention window.
    #[serde(default = "default_true")]
    pub protect_incomplete_tasks: bool,
}

/// How a recovery conflict is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictAction {
    /// Report the conflict and change nothing.
    #[default]
    Stop,
}

impl Default for BudgetSection {
    fn default() -> Self {
        Self {
            max_changed_paths: DEFAULT_MAX_CHANGED_PATHS,
            max_moves: DEFAULT_MAX_MOVES,
            max_operations: DEFAULT_MAX_OPERATIONS,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_snapshot_bytes: DEFAULT_MAX_SNAPSHOT_BYTES,
            warn_at_ratio: DEFAULT_WARN_AT_RATIO,
        }
    }
}

impl Default for ApprovalSection {
    fn default() -> Self {
        Self {
            scope_expansion: ExpansionPolicy::default(),
            budget_expansion: ExpansionPolicy::default(),
            max_elicitations_per_task: DEFAULT_MAX_ELICITATIONS_PER_TASK,
            grant_ttl_minutes: DEFAULT_GRANT_TTL_MINUTES,
        }
    }
}

impl Default for SafetySection {
    fn default() -> Self {
        Self {
            follow_symlinks: false,
            allow_cross_filesystem_move: false,
            overwrite_move_destination: false,
            create_parent_directories: false,
            workspace_writer_limit: DEFAULT_WORKSPACE_WRITER_LIMIT,
            unsafe_allow_workspace_wide: false,
        }
    }
}

impl Default for RecoverySection {
    fn default() -> Self {
        Self {
            conflict_action: ConflictAction::default(),
            retain_closed_task_days: DEFAULT_RETAIN_CLOSED_TASK_DAYS,
            protect_incomplete_tasks: true,
        }
    }
}

/// Byte offset to 1-based line number.
///
/// Built once per document so reporting many rules does not rescan the text.
#[derive(Debug)]
struct LineIndex {
    /// Byte offset at which each line starts.
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            text.bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(offset, _)| offset + 1),
        );
        Self { starts }
    }

    fn line_of(&self, offset: usize) -> u32 {
        let index = self.starts.partition_point(|&start| start <= offset);
        u32::try_from(index.max(1)).unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_handles_edges() {
        let index = LineIndex::new("a\nbb\n\nccc");
        assert_eq!(index.line_of(0), 1);
        assert_eq!(index.line_of(2), 2);
        assert_eq!(index.line_of(5), 3);
        assert_eq!(index.line_of(6), 4);
    }
}
