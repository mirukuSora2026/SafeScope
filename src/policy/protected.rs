//! Paths the engine protects unconditionally.
//!
//! Nothing here can be unlocked by a policy `allow` rule or by a scope-expansion
//! approval. That makes the list a power the user cannot take back, so it stays
//! short: every entry has to answer "why can the user not decide this?".
//!
//! Notably absent are `Cargo.lock`, `node_modules/`, `target/` and the like.
//! Those are project judgement calls and belong in a policy `deny` list, not here.

use crate::dataformatting::Msg;
use crate::error::Denial;
use crate::paths::{RelPath, TMP_PREFIX};

use super::matcher::{CaseSensitivity, Pattern};

/// Why a path is protected, so a denial can explain itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectedReason {
    /// The engine's own policy and workspace state.
    EngineState,
    /// Git history, which recovery reasoning depends on.
    GitHistory,
    /// The host's permission configuration.
    PermissionSurface,
    /// A name reserved for the atomic-replace temporary.
    TemporaryName,
}

impl ProtectedReason {
    pub const fn message(self) -> Msg {
        match self {
            ProtectedReason::EngineState => Msg::ProtectedEngineState,
            ProtectedReason::GitHistory => Msg::ProtectedGitHistory,
            ProtectedReason::PermissionSurface => Msg::ProtectedPermissionSurface,
            ProtectedReason::TemporaryName => Msg::ProtectedTemporaryName,
        }
    }
}

/// The protected patterns, paired with the reason each exists.
const ENTRIES: &[(&str, ProtectedReason)] = &[
    // The policy draft and the workspace identity. If the engine could rewrite
    // these, it could rewrite what it checks itself against.
    (".safescope/**", ProtectedReason::EngineState),
    // Recovery decides "did the user change this after we did?" against a tree
    // whose history is assumed intact.
    (".git/**", ProtectedReason::GitHistory),
    // settings.json is where tool permissions live. Write access here would let
    // the restrictions be lifted from inside.
    (".claude/**", ProtectedReason::PermissionSurface),
];

/// A protected pattern together with the reason it exists.
#[derive(Debug, Clone)]
struct ProtectedRule {
    pattern: Pattern,
    reason: ProtectedReason,
}

/// What matched, when a path turns out to be protected.
#[derive(Debug, Clone, Copy)]
pub struct ProtectedMatch<'a> {
    /// The pattern that matched, absent for the reserved-name rule, which is
    /// checked on the file name rather than by glob.
    pub pattern: Option<&'a Pattern>,
    pub reason: ProtectedReason,
}

/// The engine's fixed protected set.
#[derive(Debug, Clone)]
pub struct ProtectedPaths {
    rules: Vec<ProtectedRule>,
}

impl ProtectedPaths {
    /// Builds the engine's fixed set.
    ///
    /// # Panics
    ///
    /// Panics if a built-in pattern fails to compile, which would be a bug in
    /// this file rather than a runtime condition.
    pub fn engine_defaults() -> Self {
        let rules = ENTRIES
            .iter()
            .map(|(text, reason)| ProtectedRule {
                // Refuse broadly: on a case-insensitive filesystem `.GIT/config`
                // and `.git/config` are the same file.
                pattern: Pattern::compile(text, CaseSensitivity::AlsoIgnoringCase, None)
                    .expect("built-in protected patterns must compile"),
                reason: *reason,
            })
            .collect();
        Self { rules }
    }

    /// What protects this path, if anything.
    pub fn find(&self, path: &RelPath) -> Option<ProtectedMatch<'_>> {
        // A reserved temporary name is protected wherever it appears.
        //
        // `RelPath::parse` already refuses these, so this arm is unreachable
        // through the normal entry point. It is kept so the protection does not
        // rest on a single check: recovery scans the filesystem, and a future
        // caller may construct a path some other way.
        if path.is_engine_temporary() {
            return Some(ProtectedMatch {
                pattern: None,
                reason: ProtectedReason::TemporaryName,
            });
        }
        self.rules
            .iter()
            .find(|rule| rule.pattern.matches(path))
            .map(|rule| ProtectedMatch {
                pattern: Some(&rule.pattern),
                reason: rule.reason,
            })
    }

    pub fn matches(&self, path: &RelPath) -> bool {
        self.find(path).is_some()
    }

    /// Whether a policy pattern would try to permit something protected.
    ///
    /// Used at approval time: an `allow` rule that can never take effect is a
    /// mistake the user should be told about loudly, not one to swallow.
    pub fn conflicting_pattern(&self, allow: &Pattern) -> Option<&Pattern> {
        self.rules
            .iter()
            .map(|rule| &rule.pattern)
            .find(|protected| patterns_overlap(protected.text(), allow.text()))
    }

    /// The reserved prefix, exposed so callers can describe the rule.
    pub const fn temporary_prefix() -> &'static str {
        TMP_PREFIX
    }
}

impl Default for ProtectedPaths {
    fn default() -> Self {
        Self::engine_defaults()
    }
}

/// Conservative overlap test between two patterns.
///
/// Comparing globs exactly is undecidable in general, so this errs towards
/// reporting a conflict: it checks whether either pattern's fixed leading
/// components are a prefix of the other's. That catches `allow = [".git/**"]`
/// against `.git/**`, which is what this is for.
fn patterns_overlap(left: &str, right: &str) -> bool {
    let left_fixed = fixed_prefix(left);
    let right_fixed = fixed_prefix(right);
    if left_fixed.is_empty() || right_fixed.is_empty() {
        // One side starts with a wildcard and could reach anywhere.
        return true;
    }
    left_fixed.starts_with(&right_fixed[..]) || right_fixed.starts_with(&left_fixed[..])
}

/// The leading path components of a pattern that contain no wildcard.
fn fixed_prefix(pattern: &str) -> Vec<&str> {
    pattern
        .split('/')
        .take_while(|component| !component.contains(['*', '?', '[', '{']))
        .collect()
}

/// Builds the denial raised when a protected path is touched.
pub fn denial(reason: ProtectedReason) -> Denial {
    Denial::new(crate::error::ErrorCode::ProtectedPath, reason.message())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> RelPath {
        RelPath::parse(text).unwrap()
    }

    #[test]
    fn protects_engine_state() {
        let protected = ProtectedPaths::engine_defaults();
        let found = protected
            .find(&path(".safescope/policy.toml"))
            .expect("protected");
        assert_eq!(found.reason, ProtectedReason::EngineState);
        assert!(found.pattern.is_some());
        assert!(protected.matches(&path(".safescope/workspace-id")));
    }

    #[test]
    fn protects_git_history() {
        let protected = ProtectedPaths::engine_defaults();
        let found = protected.find(&path(".git/config")).expect("protected");
        assert_eq!(found.reason, ProtectedReason::GitHistory);
        assert!(protected.matches(&path(".git/refs/heads/main")));
    }

    #[test]
    fn protects_the_permission_surface() {
        let protected = ProtectedPaths::engine_defaults();
        let found = protected
            .find(&path(".claude/settings.json"))
            .expect("protected");
        assert_eq!(found.reason, ProtectedReason::PermissionSurface);
    }

    #[test]
    fn protection_ignores_case() {
        // On a case-insensitive filesystem .GIT/config is .git/config.
        let protected = ProtectedPaths::engine_defaults();
        assert!(protected.matches(&path(".GIT/config")));
        assert!(protected.matches(&path(".SafeScope/policy.toml")));
    }

    #[test]
    fn ordinary_project_files_are_not_protected() {
        let protected = ProtectedPaths::engine_defaults();
        assert!(!protected.matches(&path("src/main.rs")));
        assert!(!protected.matches(&path("Cargo.lock")));
        assert!(!protected.matches(&path("README.md")));
        // A file that merely starts with the same letters is a different path.
        assert!(!protected.matches(&path(".gitignore")));
        assert!(!protected.matches(&path(".claude-flow/state.json")));
    }

    #[test]
    fn an_allow_rule_over_a_protected_path_is_reported() {
        let protected = ProtectedPaths::engine_defaults();
        let offending = Pattern::compile(".git/**", CaseSensitivity::Exact, Some(4)).unwrap();
        let conflict = protected.conflicting_pattern(&offending).expect("conflict");
        assert_eq!(conflict.text(), ".git/**");
    }

    #[test]
    fn an_ordinary_allow_rule_does_not_conflict() {
        let protected = ProtectedPaths::engine_defaults();
        let ordinary = Pattern::compile("src/main/java/**", CaseSensitivity::Exact, None).unwrap();
        assert!(protected.conflicting_pattern(&ordinary).is_none());
    }

    #[test]
    fn a_workspace_wide_allow_rule_conflicts() {
        // `**` reaches everything, including protected paths.
        let protected = ProtectedPaths::engine_defaults();
        let everything = Pattern::compile("**", CaseSensitivity::Exact, None).unwrap();
        assert!(protected.conflicting_pattern(&everything).is_some());
    }

    #[test]
    fn the_reserved_name_rule_is_a_second_line_of_defence() {
        // RelPath::parse already refuses these names, so the branch in `find`
        // cannot be reached through the normal entry point — there is no way to
        // build a RelPath that would exercise it. The rule stays so the
        // protection does not rest on a single check in another module; this
        // test records that coupling so removing either side is a visible choice.
        assert!(RelPath::parse(".sfs-tmp-0001").is_err());
        assert!(RelPath::parse("src/.sfs-tmp-0001").is_err());
        assert_eq!(ProtectedPaths::temporary_prefix(), TMP_PREFIX);
    }

    #[test]
    fn every_reason_has_a_message() {
        let reasons = [
            ProtectedReason::EngineState,
            ProtectedReason::GitHistory,
            ProtectedReason::PermissionSurface,
            ProtectedReason::TemporaryName,
        ];
        for reason in reasons {
            assert!(!reason.message().to_string().trim().is_empty());
        }
    }
}
