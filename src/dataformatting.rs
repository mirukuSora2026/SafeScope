//! Message catalogue and human-readable formatting.
//!
//! Every string a person can see goes through this module. Two reasons:
//!
//! 1. **Localisation.** Messages are declared once as a typed [`Msg`] variant and
//!    rendered per language. Adding a variant breaks every language module until
//!    it is translated, so a locale can never silently fall out of date.
//! 2. **Consistency.** Byte counts and usage ratios are formatted in one place, so
//!    a limit reads the same on the status screen as it does in an error.
//!
//! Error *codes* are deliberately not translated: they are a machine contract
//! consumed by JSON output and MCP responses. The message beside a code is.

mod en;
mod ja;
mod ko;
mod ru;
mod zh;

use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

use unicode_width::UnicodeWidthStr;

/// Languages the catalogue is translated into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Language {
    #[default]
    English,
    Korean,
    Chinese,
    Japanese,
    Russian,
}

impl Language {
    pub const ALL: [Language; 5] = [
        Language::English,
        Language::Korean,
        Language::Chinese,
        Language::Japanese,
        Language::Russian,
    ];

    /// ISO 639-1 code used in configuration and environment variables.
    pub const fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Korean => "ko",
            Language::Chinese => "zh",
            Language::Japanese => "ja",
            Language::Russian => "ru",
        }
    }

    /// Parses a language tag, tolerating the shapes locale variables take:
    /// `ko`, `ko_KR`, `ko_KR.UTF-8`, `zh-Hans-CN`.
    pub fn parse(tag: &str) -> Option<Self> {
        let primary = tag
            .split(['_', '-', '.'])
            .next()
            .unwrap_or(tag)
            .trim()
            .to_ascii_lowercase();
        Language::ALL
            .into_iter()
            .find(|lang| lang.code() == primary)
    }

    /// Resolves the language from the environment, falling back to English.
    ///
    /// `SAFESCOPE_LANG` wins so a user can override a system locale without
    /// changing their shell environment.
    pub fn from_env() -> Self {
        for var in ["SAFESCOPE_LANG", "LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(value) = std::env::var(var)
                && let Some(lang) = Language::parse(&value)
            {
                return lang;
            }
        }
        Language::English
    }

    const fn from_index(index: u8) -> Self {
        match index {
            1 => Language::Korean,
            2 => Language::Chinese,
            3 => Language::Japanese,
            4 => Language::Russian,
            _ => Language::English,
        }
    }

    const fn index(self) -> u8 {
        match self {
            Language::English => 0,
            Language::Korean => 1,
            Language::Chinese => 2,
            Language::Japanese => 3,
            Language::Russian => 4,
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Process-wide language. `u8::MAX` means "not yet resolved".
static CURRENT: AtomicU8 = AtomicU8::new(u8::MAX);

/// The language messages render in. Resolved from the environment on first use.
pub fn current() -> Language {
    match CURRENT.load(Ordering::Relaxed) {
        u8::MAX => {
            let resolved = Language::from_env();
            CURRENT.store(resolved.index(), Ordering::Relaxed);
            resolved
        }
        index => Language::from_index(index),
    }
}

/// Overrides the language for this process.
pub fn set_language(language: Language) {
    CURRENT.store(language.index(), Ordering::Relaxed);
}

/// Short labels used to lay out command output.
///
/// Grouped into one [`Msg`] variant rather than one variant each, so the
/// catalogue does not acquire a dozen near-identical entries and the sample list
/// in the coverage test stays readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Label {
    Allowed,
    Refused,
    NotCovered,
    Path,
    Operation,
    Policy,
    PolicyVersion,
    EvaluationSteps,
    StepProtected,
    StepDeny,
    StepAllow,
    StepGrant,
    NoMatch,
    Outcome,
    ExpansionPossible,
    ExpansionImpossible,
    CurrentScope,
    Nothing,
    Warnings,
    Approved,
    UnapprovedEdits,
}

/// A message that can be shown to a person.
///
/// Variants carry their parameters as typed fields rather than pre-formatted
/// strings, so translations are free to reorder them.
///
/// Deliberately not `Eq`: some variants carry an `f64` for display.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Msg {
    // ── Path validation ────────────────────────────────────────────────
    PathEmpty,
    PathTooLong {
        len: usize,
        limit: usize,
    },
    PathAbsolute,
    PathEmptyComponent {
        path: String,
    },
    PathRelativeComponent {
        component: String,
    },
    PathComponentTooLong {
        len: usize,
        limit: usize,
    },
    PathControlCharacter {
        codepoint: u32,
    },
    PathBackslash,
    PathTrailingDotOrSpace {
        component: String,
    },
    PathReservedDeviceName {
        component: String,
    },
    PathReservedPrefix {
        prefix: String,
    },
    PathReservedPrefixHint,
    PathTooDeep {
        depth: usize,
        limit: usize,
    },

    // ── Policy patterns ────────────────────────────────────────────────
    PatternEmpty,
    PatternAbsolute {
        pattern: String,
    },
    PatternTraversal {
        pattern: String,
    },
    PatternInvalidGlob {
        pattern: String,
        reason: String,
    },

    // ── Policy file ────────────────────────────────────────────────────
    PolicyParseFailed {
        reason: String,
    },
    PolicySchemaUnsupported {
        found: u32,
        supported: u32,
    },

    /// A short layout label; see [`Label`].
    Label(Label),

    // ── Workspace registration ─────────────────────────────────────────
    WorkspaceAlreadyRegistered {
        root: String,
    },
    WorkspaceNotRegistered {
        root: String,
    },
    WorkspaceIdCorrupted {
        path: String,
    },
    WorkspaceRegistered {
        root: String,
        id: String,
    },
    HintRunInitFirst,
    HintFillInAllowThenApprove {
        policy: String,
    },

    // ── Journal ────────────────────────────────────────────────────────
    JournalOpenFailed {
        path: String,
        reason: String,
    },
    JournalOperationFailed {
        reason: String,
    },
    JournalRequestMismatch {
        request: String,
    },
    JournalUnknownOperation {
        operation: String,
    },

    // ── Recovery data ──────────────────────────────────────────────────
    SnapshotVerificationFailed {
        hash: String,
    },
    SnapshotMissing {
        hash: String,
    },
    SnapshotStoreFailed {
        reason: String,
    },

    // ── Engine state store ─────────────────────────────────────────────
    StoreDataDirectoryUnavailable,
    StoreWriteFailed {
        path: String,
        reason: String,
    },
    StoreReadFailed {
        path: String,
        reason: String,
    },
    StoreCorrupted {
        path: String,
        reason: String,
    },
    StoreStateInsideWorkspace {
        state: String,
        workspace: String,
    },

    // ── Platform file operations ───────────────────────────────────────
    PathDestinationExists {
        path: String,
    },
    PlatformCrossFilesystem {
        from: String,
        to: String,
    },
    PlatformAtomicRenameUnsupported {
        reason: String,
    },
    PlatformOperationFailed {
        operation: String,
        reason: String,
    },

    // ── Workspace and path resolution ──────────────────────────────────
    WorkspaceOpenFailed {
        root: String,
        reason: String,
    },
    PathNotARegularFile {
        path: String,
    },
    PathComponentNotADirectory {
        path: String,
        component: String,
    },
    PathSymlinkRefused {
        path: String,
    },
    PathParentMissing {
        path: String,
        parent: String,
    },
    HintCreateTheDirectoryFirst,

    // ── Scope evaluation ───────────────────────────────────────────────
    ScopeNotCovered {
        path: String,
    },
    ScopeDeniedByRule {
        path: String,
        pattern: String,
    },
    ScopeOperationNotAllowed {
        path: String,
        operation: String,
        allowed: String,
    },
    HintExpansionMayBeRequested,
    HintPolicyDenyIsFinal,

    // ── Policy validation ──────────────────────────────────────────────
    PolicyNoAllowRules,
    PolicyEmptyDefaultOps,
    PolicyRuleGrantsNothing {
        pattern: String,
    },
    PolicyAllowOverProtected {
        pattern: String,
        protected: String,
    },
    PolicyWorkspaceWideNeedsOptIn {
        pattern: String,
    },
    PolicyAllowAlsoDenied {
        pattern: String,
    },
    PolicyDuplicatePattern {
        pattern: String,
    },
    PolicyDenyNeverApplies {
        pattern: String,
    },
    PolicyBudgetZero {
        field: String,
    },
    PolicyFileLimitExceedsSnapshotLimit {
        file_bytes: u64,
        snapshot_bytes: u64,
    },
    PolicyWarnRatioOutOfRange {
        value: f64,
    },

    // ── Protected paths ────────────────────────────────────────────────
    ProtectedEngineState,
    ProtectedGitHistory,
    ProtectedPermissionSurface,
    ProtectedTemporaryName,

    // ── Content hash ───────────────────────────────────────────────────
    HashBadLength {
        len: usize,
    },
    HashNotHexadecimal {
        text: String,
    },

    // ── Fault injection ────────────────────────────────────────────────
    FaultUnknownValue {
        variable: String,
        value: String,
    },
    FaultAborting {
        point: String,
    },

    // ── Executable ─────────────────────────────────────────────────────
    CliNotImplemented {
        version: String,
    },
}

impl Msg {
    /// Renders this message in an explicit language.
    pub fn render(&self, language: Language) -> String {
        match language {
            Language::English => en::render(self),
            Language::Korean => ko::render(self),
            Language::Chinese => zh::render(self),
            Language::Japanese => ja::render(self),
            Language::Russian => ru::render(self),
        }
    }
}

/// Renders in the process language.
impl fmt::Display for Msg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render(current()))
    }
}

impl From<Msg> for String {
    fn from(msg: Msg) -> Self {
        msg.to_string()
    }
}

/// Pads `text` to `columns` terminal cells.
///
/// `{:<12}` counts characters, not display width, so a Korean or Japanese label
/// comes out several cells short and every column after it goes ragged. Since
/// the whole point of the catalogue is that these languages are first-class, the
/// padding has to measure what the terminal will actually draw.
pub fn pad(text: &str, columns: usize) -> String {
    let width = UnicodeWidthStr::width(text);
    let mut padded = String::with_capacity(text.len() + columns.saturating_sub(width));
    padded.push_str(text);
    for _ in width..columns {
        padded.push(' ');
    }
    padded
}

/// Formats a byte count with binary prefixes.
///
/// Configuration exposes limits such as `max_file_bytes = 8388608`; nobody reads
/// that number correctly, so it is never shown raw.
pub fn bytes(count: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if count < 1024 {
        return format!("{count} B");
    }
    let mut value = count as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    // Three significant figures reads well in a column.
    if value >= 100.0 {
        format!("{:.0} {}", value, UNITS[unit])
    } else if value >= 10.0 {
        format!("{:.1} {}", value, UNITS[unit])
    } else {
        format!("{:.2} {}", value, UNITS[unit])
    }
}

/// Formats consumption against a limit, as in `3 / 8`.
pub fn usage(used: u64, limit: u64) -> String {
    format!("{used} / {limit}")
}

/// Fraction of a limit consumed. A limit of zero counts as already exceeded.
pub fn ratio(used: u64, limit: u64) -> f64 {
    if limit == 0 {
        return f64::INFINITY;
    }
    used as f64 / limit as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_locale_shaped_tags() {
        assert_eq!(Language::parse("ko"), Some(Language::Korean));
        assert_eq!(Language::parse("ko_KR.UTF-8"), Some(Language::Korean));
        assert_eq!(Language::parse("zh-Hans-CN"), Some(Language::Chinese));
        assert_eq!(Language::parse("JA"), Some(Language::Japanese));
        assert_eq!(Language::parse("ru_RU"), Some(Language::Russian));
        assert_eq!(Language::parse("kl"), None);
    }

    #[test]
    fn language_index_round_trips() {
        for language in Language::ALL {
            assert_eq!(Language::from_index(language.index()), language);
        }
    }

    #[test]
    fn pads_by_display_width_not_character_count() {
        assert_eq!(pad("Path", 8), "Path    ");
        // Four CJK characters draw as eight cells, so no padding is owed.
        assert_eq!(pad("\u{acbd}\u{b85c}", 4), "\u{acbd}\u{b85c}");
        assert_eq!(pad("\u{acbd}\u{b85c}", 6), "\u{acbd}\u{b85c}  ");
        // Never truncates: a label longer than the column stays whole.
        assert_eq!(pad("Operation", 4), "Operation");
    }

    #[test]
    fn formats_byte_counts() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.00 KiB");
        assert_eq!(bytes(8_388_608), "8.00 MiB");
        assert_eq!(bytes(134_217_728), "128 MiB");
    }

    #[test]
    fn formats_usage_and_ratio() {
        assert_eq!(usage(3, 8), "3 / 8");
        assert!((ratio(4, 8) - 0.5).abs() < f64::EPSILON);
        assert!(ratio(1, 0).is_infinite(), "a zero limit counts as exceeded");
    }
}
