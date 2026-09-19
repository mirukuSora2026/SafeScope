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

/// A message that can be shown to a person.
///
/// Variants carry their parameters as typed fields rather than pre-formatted
/// strings, so translations are free to reorder them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Msg {
    // ── Path validation ────────────────────────────────────────────────
    PathEmpty,
    PathTooLong { len: usize, limit: usize },
    PathAbsolute,
    PathEmptyComponent { path: String },
    PathRelativeComponent { component: String },
    PathComponentTooLong { len: usize, limit: usize },
    PathControlCharacter { codepoint: u32 },
    PathBackslash,
    PathTrailingDotOrSpace { component: String },
    PathReservedDeviceName { component: String },
    PathReservedPrefix { prefix: String },
    PathReservedPrefixHint,
    PathTooDeep { depth: usize, limit: usize },

    // ── Policy patterns ────────────────────────────────────────────────
    PatternEmpty,
    PatternAbsolute { pattern: String },
    PatternTraversal { pattern: String },
    PatternInvalidGlob { pattern: String, reason: String },

    // ── Policy file ────────────────────────────────────────────────────
    PolicyParseFailed { reason: String },
    PolicySchemaUnsupported { found: u32, supported: u32 },

    // ── Protected paths ────────────────────────────────────────────────
    ProtectedEngineState,
    ProtectedGitHistory,
    ProtectedPermissionSurface,
    ProtectedTemporaryName,

    // ── Content hash ───────────────────────────────────────────────────
    HashBadLength { len: usize },
    HashNotHexadecimal { text: String },

    // ── Fault injection ────────────────────────────────────────────────
    FaultUnknownValue { variable: String, value: String },
    FaultAborting { point: String },

    // ── Executable ─────────────────────────────────────────────────────
    CliNotImplemented { version: String },
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
    fn every_language_renders_every_message() {
        // Catches a translation that returned an empty string, and — because the
        // language modules match exhaustively — a variant added without a
        // translation will not compile in the first place.
        let samples = [
            Msg::PathEmpty,
            Msg::PathTooLong {
                len: 9000,
                limit: 4096,
            },
            Msg::PathAbsolute,
            Msg::PathEmptyComponent {
                path: "a//b".into(),
            },
            Msg::PathRelativeComponent {
                component: "..".into(),
            },
            Msg::PathComponentTooLong {
                len: 300,
                limit: 255,
            },
            Msg::PathControlCharacter { codepoint: 10 },
            Msg::PathBackslash,
            Msg::PathTrailingDotOrSpace {
                component: "name.".into(),
            },
            Msg::PathReservedDeviceName {
                component: "CON".into(),
            },
            Msg::PathReservedPrefix {
                prefix: ".sfs-tmp-".into(),
            },
            Msg::PathReservedPrefixHint,
            Msg::PathTooDeep {
                depth: 65,
                limit: 64,
            },
            Msg::PatternEmpty,
            Msg::PatternAbsolute {
                pattern: "/etc/**".into(),
            },
            Msg::PatternTraversal {
                pattern: "../**".into(),
            },
            Msg::PatternInvalidGlob {
                pattern: "[".into(),
                reason: "unclosed".into(),
            },
            Msg::PolicyParseFailed {
                reason: "expected a table".into(),
            },
            Msg::PolicySchemaUnsupported {
                found: 9,
                supported: 1,
            },
            Msg::ProtectedEngineState,
            Msg::ProtectedGitHistory,
            Msg::ProtectedPermissionSurface,
            Msg::ProtectedTemporaryName,
            Msg::HashBadLength { len: 3 },
            Msg::HashNotHexadecimal { text: "zz".into() },
            Msg::FaultUnknownValue {
                variable: "SAFESCOPE_FAULT".into(),
                value: "x".into(),
            },
            Msg::FaultAborting {
                point: "after_rename_before_commit".into(),
            },
            Msg::CliNotImplemented {
                version: "0.1.0".into(),
            },
        ];
        for message in &samples {
            for language in Language::ALL {
                let text = message.render(language);
                assert!(
                    !text.trim().is_empty(),
                    "{message:?} has no {language} translation"
                );
            }
        }
    }

    #[test]
    fn translations_differ_between_languages() {
        let message = Msg::PathAbsolute;
        assert_ne!(
            message.render(Language::English),
            message.render(Language::Korean)
        );
        assert_ne!(
            message.render(Language::Chinese),
            message.render(Language::Japanese)
        );
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
