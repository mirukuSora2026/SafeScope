//! Error codes.
//!
//! The point of this module is to separate a **policy denial** from an **engine
//! fault** at the type level. Collapsing them into one error type leaves callers
//! unable to tell whether retrying is worth anything, and an AI client will
//! happily repeat a request that can never succeed.
//!
//! - [`Denial`]: policy, budget or current state said no. The same request will
//!   get the same answer however many times it is sent.
//! - [`Fault`]: the engine or its environment failed. Different conditions may
//!   produce a different outcome.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Whether retrying is meaningful.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// A rule refused the request. The request or the approval must change.
    Denial,
    /// The engine or environment failed. Conditions may change.
    Fault,
}

/// Stable error code.
///
/// These strings are part of the contract for JSON output and MCP responses, so
/// they are never renamed once published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum ErrorCode {
    // ── Scope ──────────────────────────────────────────────────────────
    /// Outside the allowed scope. A scope expansion may be requested.
    ScopeDenied,
    /// A path the engine always protects. Scope expansion cannot unlock it.
    ProtectedPath,
    /// The path is allowed but this operation is not. An expansion may be requested.
    OperationNotAllowed,
    /// The path string itself breaks the rules.
    InvalidPath,
    /// A stored or supplied content hash is malformed.
    InvalidHash,
    /// An unsupported file kind or operation: a symlink, a special file, a
    /// cross-filesystem move.
    UnsupportedOperation,
    /// The parent directory of a new file does not exist. v1 never creates directories.
    ParentMissing,

    // ── Budget ─────────────────────────────────────────────────────────
    /// The change would exceed a limit.
    BudgetExceeded,

    // ── State mismatch ─────────────────────────────────────────────────
    /// The source changed after the plan was made. The plan must be rebuilt.
    SourceChanged,
    /// A file already exists at the move destination. It is not overwritten.
    DestinationExists,
    /// The file an operation needs is not there.
    TargetMissing,
    /// A file is larger than the policy permits the engine to handle.
    FileTooLarge,
    /// Undo conflicts with a change made after the operation.
    RecoveryConflict,
    /// The outcome of an operation cannot be determined without comparing state.
    StateUncertain,

    // ── Policy and approval ────────────────────────────────────────────
    /// No approved policy exists. `safescope policy approve` is required.
    NoApprovedPolicy,
    /// The approved policy version changed after the plan was made.
    PolicyChanged,
    /// The policy file failed validation.
    PolicyInvalid,
    /// A user approval is required.
    ApprovalRequired,
    /// This approval must be given by a person at a terminal.
    ApprovalNeedsTty,

    // ── Plans and requests ─────────────────────────────────────────────
    /// No such plan.
    PlanNotFound,
    /// The plan is past its expiry.
    PlanExpired,
    /// The same request id arrived with different contents.
    RequestMismatch,

    // ── Workspace ──────────────────────────────────────────────────────
    /// Not a registered workspace.
    WorkspaceNotRegistered,
    /// Already a registered workspace.
    WorkspaceAlreadyRegistered,
    /// This project cannot be registered: its state would be unusable.
    WorkspaceStateUnusable,
    /// Another process is writing to this workspace.
    WorkspaceBusy,

    // ── Engine ─────────────────────────────────────────────────────────
    /// Recovery data could not be stored. The original was left untouched.
    SnapshotFailed,
    /// The journal could not be written.
    JournalFailed,
    /// A filesystem operation failed.
    IoFailed,
    /// An engine invariant was violated. This is a bug.
    Internal,
}

impl ErrorCode {
    /// The stable string used in JSON and MCP responses.
    pub const fn as_str(self) -> &'static str {
        use ErrorCode::*;
        match self {
            ScopeDenied => "SCOPE_DENIED",
            ProtectedPath => "PROTECTED_PATH",
            OperationNotAllowed => "OPERATION_NOT_ALLOWED",
            InvalidPath => "INVALID_PATH",
            InvalidHash => "INVALID_HASH",
            UnsupportedOperation => "UNSUPPORTED_OPERATION",
            ParentMissing => "PARENT_MISSING",
            BudgetExceeded => "BUDGET_EXCEEDED",
            SourceChanged => "SOURCE_CHANGED",
            DestinationExists => "DESTINATION_EXISTS",
            TargetMissing => "TARGET_MISSING",
            FileTooLarge => "FILE_TOO_LARGE",
            RecoveryConflict => "RECOVERY_CONFLICT",
            StateUncertain => "STATE_UNCERTAIN",
            NoApprovedPolicy => "NO_APPROVED_POLICY",
            PolicyChanged => "POLICY_CHANGED",
            PolicyInvalid => "POLICY_INVALID",
            ApprovalRequired => "APPROVAL_REQUIRED",
            ApprovalNeedsTty => "APPROVAL_NEEDS_TTY",
            PlanNotFound => "PLAN_NOT_FOUND",
            PlanExpired => "PLAN_EXPIRED",
            RequestMismatch => "REQUEST_MISMATCH",
            WorkspaceNotRegistered => "WORKSPACE_NOT_REGISTERED",
            WorkspaceAlreadyRegistered => "WORKSPACE_ALREADY_REGISTERED",
            WorkspaceStateUnusable => "WORKSPACE_STATE_UNUSABLE",
            WorkspaceBusy => "WORKSPACE_BUSY",
            SnapshotFailed => "SNAPSHOT_FAILED",
            JournalFailed => "JOURNAL_FAILED",
            IoFailed => "IO_FAILED",
            Internal => "INTERNAL",
        }
    }

    /// Whether this is a refusal or a failure.
    pub const fn kind(self) -> ErrorKind {
        use ErrorCode::*;
        match self {
            ScopeDenied
            | ProtectedPath
            | OperationNotAllowed
            | InvalidPath
            | InvalidHash
            | UnsupportedOperation
            | ParentMissing
            | BudgetExceeded
            | SourceChanged
            | DestinationExists
            | TargetMissing
            | FileTooLarge
            | RecoveryConflict
            | NoApprovedPolicy
            | PolicyChanged
            | PolicyInvalid
            | ApprovalRequired
            | ApprovalNeedsTty
            | PlanNotFound
            | PlanExpired
            | RequestMismatch
            | WorkspaceNotRegistered
            | WorkspaceAlreadyRegistered
            | WorkspaceStateUnusable => ErrorKind::Denial,

            StateUncertain | WorkspaceBusy | SnapshotFailed | JournalFailed | IoFailed
            | Internal => ErrorKind::Fault,
        }
    }

    /// Whether resending the identical request is worth anything.
    ///
    /// Every denial is `false`. Several faults are `false` too: when state needs
    /// comparing or a person needs to intervene, an automatic retry makes things
    /// worse rather than better.
    pub const fn retryable(self) -> bool {
        matches!(self, ErrorCode::WorkspaceBusy)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A rule refused the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denial {
    code: ErrorCode,
    message: String,
    /// What the caller can do next.
    ///
    /// This is the field that stops an AI client from repeating a request that
    /// will never be allowed, so it matters more than it looks.
    hint: Option<String>,
}

impl Denial {
    /// # Panics
    ///
    /// Panics in debug builds if `code` is not a denial code; a miscategorised
    /// code is a bug rather than a runtime condition.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        debug_assert_eq!(
            code.kind(),
            ErrorKind::Denial,
            "{code} is not a denial code"
        );
        Self {
            code,
            message: message.into(),
            hint: None,
        }
    }

    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }
}

impl fmt::Display for Denial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Denial {}

/// The engine or its environment failed.
#[derive(Debug)]
pub struct Fault {
    code: ErrorCode,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl Fault {
    /// # Panics
    ///
    /// Panics in debug builds if `code` is not a fault code.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        debug_assert_eq!(code.kind(), ErrorKind::Fault, "{code} is not a fault code");
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    #[must_use]
    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    pub fn io(message: impl Into<String>, source: std::io::Error) -> Self {
        Self::new(ErrorCode::IoFailed, message).with_source(source)
    }

    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Fault {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|error| &**error as &(dyn std::error::Error + 'static))
    }
}

/// The engine-wide error type.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Denied(#[from] Denial),
    #[error(transparent)]
    Faulted(#[from] Fault),
}

impl Error {
    pub const fn code(&self) -> ErrorCode {
        match self {
            Error::Denied(denial) => denial.code(),
            Error::Faulted(fault) => fault.code(),
        }
    }

    pub const fn kind(&self) -> ErrorKind {
        self.code().kind()
    }

    pub const fn is_denial(&self) -> bool {
        matches!(self, Error::Denied(_))
    }

    /// The serialisable form used by `--json` output and MCP responses.
    pub fn report(&self) -> ErrorReport {
        let code = self.code();
        let (message, hint) = match self {
            Error::Denied(denial) => (
                denial.message().to_owned(),
                denial.hint().map(str::to_owned),
            ),
            Error::Faulted(fault) => (fault.message().to_owned(), None),
        };
        ErrorReport {
            code: code.as_str(),
            kind: code.kind(),
            retryable: code.retryable(),
            message,
            hint,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Faulted(Fault::io("filesystem operation failed", error))
    }
}

/// The machine-readable form of an error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorReport {
    pub code: &'static str,
    pub kind: ErrorKind,
    /// Whether resending the identical request is worth anything.
    pub retryable: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// Convenience alias used throughout the crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denials_are_never_retryable() {
        let denials = [
            ErrorCode::ScopeDenied,
            ErrorCode::ProtectedPath,
            ErrorCode::BudgetExceeded,
            ErrorCode::SourceChanged,
            ErrorCode::PlanExpired,
        ];
        for code in denials {
            assert_eq!(code.kind(), ErrorKind::Denial);
            assert!(!code.retryable(), "{code} must not be marked retryable");
        }
    }

    #[test]
    fn snapshot_failure_is_a_fault_but_not_retryable() {
        // Retrying automatically on a full disk only makes the situation worse.
        assert_eq!(ErrorCode::SnapshotFailed.kind(), ErrorKind::Fault);
        assert!(!ErrorCode::SnapshotFailed.retryable());
    }

    #[test]
    fn code_strings_match_the_variant_names() {
        assert_eq!(ErrorCode::ScopeDenied.as_str(), "SCOPE_DENIED");
        assert_eq!(
            ErrorCode::OperationNotAllowed.as_str(),
            "OPERATION_NOT_ALLOWED"
        );
        let json = serde_json::to_string(&ErrorCode::ProtectedPath).unwrap();
        assert_eq!(json, "\"PROTECTED_PATH\"");
    }

    #[test]
    fn hints_reach_the_report() {
        let error: Error = Denial::new(ErrorCode::ScopeDenied, "outside the allowed scope")
            .with_hint("Do not retry the same edit.")
            .into();
        let report = error.report();
        assert_eq!(report.code, "SCOPE_DENIED");
        assert!(!report.retryable);
        assert_eq!(report.hint.as_deref(), Some("Do not retry the same edit."));
    }
}
