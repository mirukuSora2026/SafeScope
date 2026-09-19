//! Validated workspace-relative paths.
//!
//! Every path decision in the engine goes through [`RelPath`]. If an evaluation
//! function or the executor accepted a `&str`, sooner or later somebody would
//! hand it an unvalidated string, and that single call would undo the boundary.
//! Keeping the constructor in this module is the defence.
//!
//! **Paths are rejected, not normalised.** Rather than resolving `..` to flatten
//! a path, the parser refuses it. Normalisation logic grows bugs; refusal does not.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

use crate::dataformatting::Msg;
use crate::error::{Denial, ErrorCode};

/// Maximum bytes in a single component, matching most filesystems.
pub const MAX_COMPONENT_BYTES: usize = 255;
/// Maximum bytes in the whole relative path.
pub const MAX_PATH_BYTES: usize = 4096;
/// Maximum nesting depth. A defensive ceiling rather than a filesystem limit.
pub const MAX_DEPTH: usize = 64;

/// Prefix the engine uses for the temporary file behind an atomic replace.
///
/// User files must not be able to take this name, otherwise recovery could not
/// safely clean up temporaries left behind by a crash.
pub const TMP_PREFIX: &str = ".sfs-tmp-";

/// Reserved Windows device names.
///
/// Only macOS and Linux are supported today, but rejecting these now keeps
/// existing journals replayable if Windows support is added later.
const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A validated path relative to the workspace root.
///
/// Invariants:
///
/// - non-empty and not absolute
/// - no component is `.`, `..` or empty
/// - no component contains a control character, NUL, `/` or `\`
/// - no component ends with `.` or a space
/// - no component is a reserved device name
/// - no component uses the engine's temporary-file prefix
/// - length and depth are within limits
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RelPath {
    /// The original text, used for filesystem access and for the journal.
    text: String,
    /// Byte offset of each component within `text`.
    starts: Vec<u32>,
    /// NFC-normalised form, used only for pattern matching.
    ///
    /// macOS can hand back decomposed (NFD) names while a policy pattern was
    /// written composed (NFC). Matching on this field keeps the two in step;
    /// storage and filesystem access always use `text`.
    match_key: String,
}

impl RelPath {
    /// Validates a caller-supplied string. This is the only way to build one.
    pub fn parse(input: &str) -> Result<Self, Denial> {
        if input.is_empty() {
            return Err(invalid(Msg::PathEmpty));
        }
        if input.len() > MAX_PATH_BYTES {
            return Err(invalid(Msg::PathTooLong {
                len: input.len(),
                limit: MAX_PATH_BYTES,
            }));
        }
        if input.starts_with('/') || looks_like_windows_absolute(input) {
            return Err(invalid(Msg::PathAbsolute));
        }

        let mut starts = Vec::new();
        let mut offset = 0usize;
        for component in input.split('/') {
            validate_component(component, input)?;
            let start = u32::try_from(offset).map_err(|_| {
                invalid(Msg::PathTooLong {
                    len: input.len(),
                    limit: MAX_PATH_BYTES,
                })
            })?;
            starts.push(start);
            offset += component.len() + 1; // component plus its separator
        }
        if starts.len() > MAX_DEPTH {
            return Err(invalid(Msg::PathTooDeep {
                depth: starts.len(),
                limit: MAX_DEPTH,
            }));
        }

        let match_key = to_nfc(input);
        Ok(Self {
            text: input.to_owned(),
            starts,
            match_key,
        })
    }

    /// The original path text. Used for filesystem access and journalling.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The NFC-normalised form, used only for policy pattern matching.
    pub fn match_key(&self) -> &str {
        &self.match_key
    }

    /// The path components, in order.
    pub fn components(&self) -> impl ExactSizeIterator<Item = &str> {
        let text = &self.text;
        let starts = &self.starts;
        (0..starts.len()).map(move |index| {
            let begin = starts[index] as usize;
            let end = starts
                .get(index + 1)
                .map_or(text.len(), |&next| next as usize - 1);
            &text[begin..end]
        })
    }

    /// Number of components.
    pub fn depth(&self) -> usize {
        self.starts.len()
    }

    /// The final component.
    pub fn file_name(&self) -> &str {
        let begin = *self
            .starts
            .last()
            .expect("invariant: at least one component") as usize;
        &self.text[begin..]
    }

    /// The parent path, or `None` for a top-level entry, which would mean the
    /// workspace root itself.
    pub fn parent(&self) -> Option<Self> {
        if self.starts.len() < 2 {
            return None;
        }
        let end = *self.starts.last().expect("checked above") as usize - 1;
        let text = self.text[..end].to_owned();
        Some(Self {
            match_key: to_nfc(&text),
            starts: self.starts[..self.starts.len() - 1].to_vec(),
            text,
        })
    }

    /// Whether this names one of the engine's temporary files, which recovery
    /// is allowed to clean up.
    pub fn is_engine_temporary(&self) -> bool {
        self.file_name().starts_with(TMP_PREFIX)
    }
}

impl fmt::Display for RelPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl Serialize for RelPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text)
    }
}

/// Stored records are re-validated on the way back in, so a corrupted or
/// hand-edited journal cannot feed an unchecked path into the executor.
impl<'de> Deserialize<'de> for RelPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        RelPath::parse(&raw).map_err(serde::de::Error::custom)
    }
}

fn validate_component(component: &str, whole: &str) -> Result<(), Denial> {
    if component.is_empty() {
        return Err(invalid(Msg::PathEmptyComponent {
            path: whole.to_owned(),
        }));
    }
    if component == "." || component == ".." {
        return Err(invalid(Msg::PathRelativeComponent {
            component: component.to_owned(),
        }));
    }
    if component.len() > MAX_COMPONENT_BYTES {
        return Err(invalid(Msg::PathComponentTooLong {
            len: component.len(),
            limit: MAX_COMPONENT_BYTES,
        }));
    }
    if let Some(bad) = component.chars().find(|character| character.is_control()) {
        return Err(invalid(Msg::PathControlCharacter {
            codepoint: bad as u32,
        }));
    }
    if component.contains('\\') {
        return Err(invalid(Msg::PathBackslash));
    }
    // Windows silently trims a trailing dot or space, which would make the name
    // that was checked differ from the name that gets created.
    if component.ends_with('.') || component.ends_with(' ') {
        return Err(invalid(Msg::PathTrailingDotOrSpace {
            component: component.to_owned(),
        }));
    }
    if is_windows_reserved(component) {
        return Err(invalid(Msg::PathReservedDeviceName {
            component: component.to_owned(),
        }));
    }
    if component.starts_with(TMP_PREFIX) {
        return Err(invalid(Msg::PathReservedPrefix {
            prefix: TMP_PREFIX.to_owned(),
        })
        .with_hint(Msg::PathReservedPrefixHint));
    }
    Ok(())
}

fn is_windows_reserved(component: &str) -> bool {
    // Not just "CON" — "CON.txt" is reserved too.
    let stem = component.split('.').next().unwrap_or(component);
    WINDOWS_RESERVED
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

fn looks_like_windows_absolute(input: &str) -> bool {
    let bytes = input.as_bytes();
    bytes.starts_with(br"\\")
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

/// Normalises to NFC, avoiding a copy when the input already is.
fn to_nfc(text: &str) -> String {
    match is_nfc_quick(text.chars()) {
        IsNormalized::Yes => text.to_owned(),
        _ => text.nfc().collect(),
    }
}

fn invalid(message: Msg) -> Denial {
    Denial::new(ErrorCode::InvalidPath, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepted(input: &str) -> RelPath {
        RelPath::parse(input)
            .unwrap_or_else(|error| panic!("{input:?} should be accepted: {error}"))
    }

    fn rejected(input: &str) {
        assert!(
            RelPath::parse(input).is_err(),
            "{input:?} should be rejected"
        );
    }

    #[test]
    fn accepts_an_ordinary_path() {
        let path = accepted("src/main/java/auth/LoginService.java");
        assert_eq!(path.depth(), 5);
        assert_eq!(path.file_name(), "LoginService.java");
        assert_eq!(path.as_str(), "src/main/java/auth/LoginService.java");
    }

    #[test]
    fn accepts_a_top_level_file() {
        let path = accepted("README.md");
        assert_eq!(path.depth(), 1);
        assert_eq!(path.file_name(), "README.md");
        assert!(path.parent().is_none());
    }

    #[test]
    fn accepts_dot_prefixed_names() {
        assert_eq!(accepted(".env").file_name(), ".env");
        assert_eq!(accepted("config/.gitkeep").depth(), 2);
    }

    #[test]
    fn rejects_traversal_rather_than_resolving_it() {
        rejected("../outside.txt");
        rejected("src/../../etc/passwd");
        rejected("src/./main.rs");
        rejected("..");
        rejected(".");
    }

    #[test]
    fn rejects_absolute_paths() {
        rejected("/etc/passwd");
        rejected(r"C:\Windows\system32");
        rejected(r"\\server\share");
    }

    #[test]
    fn rejects_empty_components() {
        rejected("src//main.rs");
        rejected("/src/main.rs");
        rejected("src/main.rs/");
        rejected("");
    }

    #[test]
    fn rejects_control_characters_and_backslashes() {
        rejected("src/main\n.rs");
        rejected("src/main\0.rs");
        rejected(r"src\main.rs");
    }

    #[test]
    fn rejects_names_windows_would_rewrite() {
        rejected("src/main.rs.");
        rejected("src/trailing ");
        rejected("src/CON");
        rejected("src/con.txt");
        rejected("NUL");
    }

    #[test]
    fn rejects_the_engine_temporary_prefix() {
        rejected(".sfs-tmp-abc");
        rejected("src/.sfs-tmp-0001");
    }

    #[test]
    fn strips_one_level_per_parent_call() {
        let path = accepted("a/b/c.txt");
        let parent = path.parent().expect("a/b");
        assert_eq!(parent.as_str(), "a/b");
        assert_eq!(parent.file_name(), "b");

        let grandparent = parent.parent().expect("a");
        assert_eq!(grandparent.as_str(), "a");
        assert!(grandparent.parent().is_none());
    }

    #[test]
    fn splits_components_exactly() {
        let path = accepted("src/main/java/A.java");
        let components: Vec<_> = path.components().collect();
        assert_eq!(components, ["src", "main", "java", "A.java"]);
        assert_eq!(path.components().len(), 4);
    }

    #[test]
    fn keeps_the_original_form_while_matching_on_nfc() {
        // "é" written decomposed, the shape macOS can hand back.
        let decomposed = "src/cafe\u{301}.txt";
        let path = accepted(decomposed);
        assert_eq!(
            path.as_str(),
            decomposed,
            "the original must survive for filesystem access"
        );
        assert_eq!(path.match_key(), "src/caf\u{e9}.txt", "matching uses NFC");
    }

    #[test]
    fn deserialisation_revalidates() {
        let good: RelPath = serde_json::from_str("\"src/a.rs\"").unwrap();
        assert_eq!(good.as_str(), "src/a.rs");
        assert!(
            serde_json::from_str::<RelPath>("\"../escape\"").is_err(),
            "a corrupted journal must not reach the executor"
        );
    }

    #[test]
    fn enforces_depth_and_length_limits() {
        rejected(&vec!["a"; MAX_DEPTH + 1].join("/"));
        rejected(&"x".repeat(MAX_COMPONENT_BYTES + 1));
    }

    #[test]
    fn recognises_engine_temporaries() {
        // The parser refuses to build one, so this is checked on a name that
        // recovery would encounter by scanning the filesystem instead.
        let ordinary = accepted("src/main.rs");
        assert!(!ordinary.is_engine_temporary());
    }
}
