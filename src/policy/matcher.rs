//! Glob pattern matching for policy rules.
//!
//! Two decisions in this module carry most of the weight.
//!
//! **`literal_separator` is always on.** Without it `*` matches across `/`, which
//! quietly turns `src/*` into `src/**` and widens every rule written with a single
//! star. A test pins this.
//!
//! **Matching runs on [`RelPath::match_key`], never on a raw string.** The key is
//! NFC-normalised and built from validated components, so the classic prefix
//! confusion — `src` matching `src-backup` — cannot arise: a separator-aware glob
//! compares components, not bytes.

use globset::{GlobBuilder, GlobMatcher};

use crate::dataformatting::Msg;
use crate::error::{Denial, ErrorCode};
use crate::paths::RelPath;

/// How a pattern treats letter case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseSensitivity {
    /// Matches exactly. Used for `allow`, so permission is granted narrowly.
    Exact,
    /// Matches exactly *and* ignoring case. Used for `deny` and protected paths,
    /// so refusal is broad.
    ///
    /// macOS is case-insensitive by default, where `.ENV` and `.env` are the same
    /// file. An exact-only deny rule would be trivially sidestepped.
    AlsoIgnoringCase,
}

/// One compiled policy pattern.
#[derive(Debug, Clone)]
pub struct Pattern {
    text: String,
    line: Option<u32>,
    exact: GlobMatcher,
    ignoring_case: Option<GlobMatcher>,
}

impl Pattern {
    /// Validates and compiles a pattern.
    ///
    /// `line` is the 1-based line in the policy file, carried so a denial can say
    /// which rule produced it.
    pub fn compile(text: &str, case: CaseSensitivity, line: Option<u32>) -> Result<Self, Denial> {
        if text.trim().is_empty() {
            return Err(invalid(Msg::PatternEmpty));
        }
        if text.starts_with('/') || looks_absolute(text) {
            return Err(invalid(Msg::PatternAbsolute {
                pattern: text.to_owned(),
            }));
        }
        if text.split('/').any(|component| component == "..") {
            return Err(invalid(Msg::PatternTraversal {
                pattern: text.to_owned(),
            }));
        }

        let exact = build(text, false)?;
        let ignoring_case = match case {
            CaseSensitivity::Exact => None,
            CaseSensitivity::AlsoIgnoringCase => Some(build(text, true)?),
        };
        Ok(Self {
            text: text.to_owned(),
            line,
            exact,
            ignoring_case,
        })
    }

    /// Whether this pattern covers the given path.
    pub fn matches(&self, path: &RelPath) -> bool {
        let key = path.match_key();
        self.exact.is_match(key)
            || self
                .ignoring_case
                .as_ref()
                .is_some_and(|glob| glob.is_match(key))
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub const fn line(&self) -> Option<u32> {
        self.line
    }
}

/// An ordered collection of patterns that reports which one matched.
///
/// Scanning linearly rather than using a compiled `GlobSet` keeps the matching
/// rule identifiable without an allocation. Policies hold tens of patterns, not
/// thousands; revisit if that stops being true.
#[derive(Debug, Clone, Default)]
pub struct PatternSet {
    patterns: Vec<Pattern>,
}

impl PatternSet {
    pub fn new(patterns: Vec<Pattern>) -> Self {
        Self { patterns }
    }

    /// Compiles every pattern in one go, failing on the first bad one.
    pub fn compile<'a>(
        texts: impl IntoIterator<Item = (&'a str, Option<u32>)>,
        case: CaseSensitivity,
    ) -> Result<Self, Denial> {
        texts
            .into_iter()
            .map(|(text, line)| Pattern::compile(text, case, line))
            .collect::<Result<Vec<_>, _>>()
            .map(Self::new)
    }

    /// The first pattern covering this path, in declaration order.
    pub fn find(&self, path: &RelPath) -> Option<&Pattern> {
        self.patterns.iter().find(|pattern| pattern.matches(path))
    }

    pub fn matches(&self, path: &RelPath) -> bool {
        self.find(path).is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Pattern> {
        self.patterns.iter()
    }

    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }
}

fn build(text: &str, ignore_case: bool) -> Result<GlobMatcher, Denial> {
    GlobBuilder::new(text)
        // Without this, `*` swallows `/` and every single-star rule silently
        // becomes recursive.
        .literal_separator(true)
        .case_insensitive(ignore_case)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|error| {
            invalid(Msg::PatternInvalidGlob {
                pattern: text.to_owned(),
                reason: error.to_string(),
            })
        })
}

fn looks_absolute(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.starts_with(br"\\")
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

fn invalid(message: Msg) -> Denial {
    Denial::new(ErrorCode::PolicyInvalid, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> RelPath {
        RelPath::parse(text).unwrap()
    }

    fn exact(text: &str) -> Pattern {
        Pattern::compile(text, CaseSensitivity::Exact, None).unwrap()
    }

    fn insensitive(text: &str) -> Pattern {
        Pattern::compile(text, CaseSensitivity::AlsoIgnoringCase, None).unwrap()
    }

    #[test]
    fn a_single_star_does_not_cross_a_separator() {
        // The whole reason literal_separator is set. If this regresses, every
        // rule written with one star silently becomes recursive.
        let pattern = exact("src/*");
        assert!(pattern.matches(&path("src/main.rs")));
        assert!(!pattern.matches(&path("src/deep/main.rs")));
        assert!(!pattern.matches(&path("src/a/b/c/d.rs")));
    }

    #[test]
    fn a_double_star_is_recursive() {
        let pattern = exact("src/auth/**");
        assert!(pattern.matches(&path("src/auth/Login.java")));
        assert!(pattern.matches(&path("src/auth/token/Jwt.java")));
        assert!(pattern.matches(&path("src/auth/a/b/c/Deep.java")));
    }

    #[test]
    fn a_prefix_is_not_a_directory() {
        // The failure mode a naive starts_with check would have: src-backup is
        // not inside src.
        let pattern = exact("src/**");
        assert!(pattern.matches(&path("src/Main.java")));
        assert!(!pattern.matches(&path("src-backup/Main.java")));
        assert!(!pattern.matches(&path("srcx/Main.java")));
    }

    #[test]
    fn leading_double_star_reaches_the_root() {
        // Deny rules are written this way, so a top-level .env has to match.
        let pattern = exact("**/.env");
        assert!(pattern.matches(&path(".env")));
        assert!(pattern.matches(&path("config/.env")));
        assert!(pattern.matches(&path("a/b/c/.env")));
        assert!(!pattern.matches(&path("config/.env.local")));
    }

    #[test]
    fn suffix_wildcards_work_within_a_component() {
        let pattern = exact("**/.env.*");
        assert!(pattern.matches(&path(".env.local")));
        assert!(pattern.matches(&path("config/.env.production")));
        assert!(!pattern.matches(&path(".env")));
    }

    #[test]
    fn alternation_and_character_classes_compile() {
        let pattern = exact("src/**/*.{java,kt}");
        assert!(pattern.matches(&path("src/auth/Login.java")));
        assert!(pattern.matches(&path("src/auth/Login.kt")));
        assert!(!pattern.matches(&path("src/auth/Login.rs")));
    }

    #[test]
    fn exact_patterns_respect_case() {
        let pattern = exact("**/.env");
        assert!(pattern.matches(&path(".env")));
        assert!(!pattern.matches(&path(".ENV")));
    }

    #[test]
    fn deny_patterns_also_match_ignoring_case() {
        // macOS treats .ENV and .env as one file; a case-sensitive deny rule
        // would be trivial to sidestep.
        let pattern = insensitive("**/.env");
        assert!(pattern.matches(&path(".env")));
        assert!(pattern.matches(&path(".ENV")));
        assert!(pattern.matches(&path("config/.Env")));
    }

    #[test]
    fn matching_uses_the_normalised_key() {
        // The path arrives decomposed, the pattern is written composed.
        let pattern = exact("src/caf\u{e9}.txt");
        assert!(pattern.matches(&path("src/cafe\u{301}.txt")));
    }

    #[test]
    fn rejects_unusable_patterns() {
        assert!(Pattern::compile("", CaseSensitivity::Exact, None).is_err());
        assert!(Pattern::compile("   ", CaseSensitivity::Exact, None).is_err());
        assert!(Pattern::compile("/etc/**", CaseSensitivity::Exact, None).is_err());
        assert!(Pattern::compile(r"C:\x", CaseSensitivity::Exact, None).is_err());
        assert!(Pattern::compile("../outside/**", CaseSensitivity::Exact, None).is_err());
        assert!(Pattern::compile("src/../etc", CaseSensitivity::Exact, None).is_err());
    }

    #[test]
    fn rejected_patterns_are_policy_errors() {
        let denial = Pattern::compile("/etc/**", CaseSensitivity::Exact, None).unwrap_err();
        assert_eq!(denial.code(), ErrorCode::PolicyInvalid);
    }

    #[test]
    fn a_set_reports_the_first_matching_pattern() {
        let set = PatternSet::compile(
            [("src/**", Some(10)), ("docs/**", Some(11))],
            CaseSensitivity::Exact,
        )
        .unwrap();

        let matched = set.find(&path("src/Main.java")).expect("src matches");
        assert_eq!(matched.text(), "src/**");
        assert_eq!(matched.line(), Some(10));

        assert!(set.find(&path("other/Main.java")).is_none());
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn an_empty_set_matches_nothing() {
        let set = PatternSet::default();
        assert!(set.is_empty());
        assert!(!set.matches(&path("anything.txt")));
    }

    #[test]
    fn compilation_fails_on_the_first_bad_pattern() {
        let result =
            PatternSet::compile([("src/**", None), ("../bad", None)], CaseSensitivity::Exact);
        assert!(result.is_err());
    }
}
