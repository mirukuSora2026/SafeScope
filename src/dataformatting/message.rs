//! The message catalogue's data: every string a person can see, as a type.
//!
//! Split from the rendering machinery so the catalogue can keep growing without
//! the module that dispatches it growing too.

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

    // ── Budget ─────────────────────────────────────────────────────────
    BudgetPathsExceeded {
        used: u64,
        limit: u64,
        adding: String,
    },
    BudgetOperationsExceeded {
        used: u64,
        limit: u64,
    },
    BudgetMovesExceeded {
        used: u64,
        limit: u64,
    },
    BudgetStorageExceeded {
        used: String,
        limit: String,
    },
    HintRequestBudgetExpansion,

    // ── Planning ───────────────────────────────────────────────────────
    PlanTargetExists {
        path: String,
    },
    PlanTargetMissing {
        path: String,
    },
    PlanFileTooLarge {
        path: String,
        size: String,
        limit: String,
    },
    PlanHasExpired {
        plan: String,
    },
    PlanStateChanged {
        path: String,
    },
    HintRebuildThePlan,

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
