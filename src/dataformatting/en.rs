//! English messages.

use crate::dataformatting::Msg;

pub(super) fn render(msg: &Msg) -> String {
    match msg {
        Msg::PathEmpty => "The path is empty.".to_owned(),
        Msg::PathTooLong { len, limit } => {
            format!("The path is too long ({len} bytes; the limit is {limit}).")
        }
        Msg::PathAbsolute => {
            "Absolute paths are not accepted. Use a path relative to the workspace root.".to_owned()
        }
        Msg::PathEmptyComponent { path } => format!(
            "The path has an empty component; check for a leading, trailing or doubled '/': {path:?}"
        ),
        Msg::PathRelativeComponent { component } => format!(
            "The '{component}' component is not allowed. Only flat paths relative to the \
             workspace root are accepted."
        ),
        Msg::PathComponentTooLong { len, limit } => {
            format!("A path component is too long ({len} bytes; the limit is {limit}).")
        }
        Msg::PathControlCharacter { codepoint } => {
            format!("The path contains a control character (U+{codepoint:04X}).")
        }
        Msg::PathBackslash => {
            "A path component may not contain '\\'. The separator is '/'.".to_owned()
        }
        Msg::PathTrailingDotOrSpace { component } => {
            format!("A path component may not end with '.' or a space: {component:?}")
        }
        Msg::PathReservedDeviceName { component } => {
            format!("'{component}' is a reserved device name.")
        }
        Msg::PathReservedPrefix { prefix } => {
            format!("Names beginning with '{prefix}' are reserved by the engine.")
        }
        Msg::PathReservedPrefixHint => "Choose a different file name.".to_owned(),
        Msg::PathTooDeep { depth, limit } => {
            format!("The path is too deep ({depth} levels; the limit is {limit}).")
        }
        Msg::PatternEmpty => "A policy pattern is empty.".to_owned(),
        Msg::PatternAbsolute { pattern } => format!(
            "The policy pattern {pattern:?} is absolute. Patterns are relative to the \
             workspace root."
        ),
        Msg::PatternTraversal { pattern } => {
            format!("The policy pattern {pattern:?} contains '..', which is not allowed.")
        }
        Msg::PatternInvalidGlob { pattern, reason } => {
            format!("The policy pattern {pattern:?} is not a valid glob: {reason}")
        }
        Msg::ProtectedEngineState => "The engine's own policy and workspace state. \
Changing it would let the engine rewrite what it checks against."
            .to_owned(),
        Msg::ProtectedGitHistory => "Git history. Recovery reasoning assumes it is intact; \
if it can be rewritten, conflict decisions lose their meaning."
            .to_owned(),
        Msg::ProtectedPermissionSurface => "Claude Code's permission settings. Write access \
here would allow the tool restrictions to be lifted from inside."
            .to_owned(),
        Msg::ProtectedTemporaryName => {
            "A name reserved for the engine's atomic replace.".to_owned()
        }
        Msg::HashBadLength { len } => {
            format!("The hash has the wrong length ({len} characters; 64 are required).")
        }
        Msg::HashNotHexadecimal { text } => {
            format!("The hash contains a non-hexadecimal value: {text:?}")
        }
        Msg::FaultUnknownValue { variable, value } => {
            format!("Unrecognised {variable} value: {value:?}")
        }
        Msg::FaultAborting { point } => {
            format!("Aborting at injected fault point {point}.")
        }
        Msg::CliNotImplemented { version } => {
            format!("safescope {version} — the command line is not built yet (planned for M1).")
        }
    }
}
