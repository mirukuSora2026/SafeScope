//! Every message renders in every language.
//!
//! Lives outside the crate so the sample list can grow with the catalogue
//! without pushing `dataformatting.rs` past the project's file-length limit.
//!
//! The language modules match exhaustively, so a variant added without a
//! translation fails to compile. What this file adds is the check that a
//! translation is not merely present but non-empty, and that the languages
//! actually differ.

use safescope::dataformatting::{Label, Language, Msg};

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
        Msg::Label(Label::Allowed),
        Msg::WorkspaceAlreadyRegistered { root: "/p".into() },
        Msg::WorkspaceNotRegistered { root: "/p".into() },
        Msg::WorkspaceIdCorrupted {
            path: "/p/.safescope/workspace-id".into(),
        },
        Msg::WorkspaceRegistered {
            root: "/p".into(),
            id: "ws_1".into(),
        },
        Msg::HintRunInitFirst,
        Msg::HintFillInAllowThenApprove {
            policy: ".safescope/policy.toml".into(),
        },
        Msg::PlanTargetExists {
            path: "a.rs".into(),
        },
        Msg::PlanTargetMissing {
            path: "a.rs".into(),
        },
        Msg::PlanFileTooLarge {
            path: "a.rs".into(),
            size: "9 MiB".into(),
            limit: "8 MiB".into(),
        },
        Msg::PlanHasExpired {
            plan: "plan_1".into(),
        },
        Msg::PlanStateChanged {
            path: "a.rs".into(),
        },
        Msg::HintRebuildThePlan,
        Msg::JournalOpenFailed {
            path: "/s/j".into(),
            reason: "locked".into(),
        },
        Msg::JournalOperationFailed {
            reason: "disk full".into(),
        },
        Msg::JournalRequestMismatch {
            request: "req_1".into(),
        },
        Msg::JournalUnknownOperation {
            operation: "op_1".into(),
        },
        Msg::SnapshotVerificationFailed { hash: "abc".into() },
        Msg::SnapshotMissing { hash: "abc".into() },
        Msg::SnapshotStoreFailed {
            reason: "disk full".into(),
        },
        Msg::StoreDataDirectoryUnavailable,
        Msg::StoreWriteFailed {
            path: "/s/x".into(),
            reason: "disk full".into(),
        },
        Msg::StoreReadFailed {
            path: "/s/x".into(),
            reason: "denied".into(),
        },
        Msg::StoreCorrupted {
            path: "/s/x".into(),
            reason: "bad json".into(),
        },
        Msg::StoreStateInsideWorkspace {
            state: "/p/.s".into(),
            workspace: "/p".into(),
        },
        Msg::PathDestinationExists {
            path: "b.txt".into(),
        },
        Msg::PlatformCrossFilesystem {
            from: "a".into(),
            to: "b".into(),
        },
        Msg::PlatformAtomicRenameUnsupported {
            reason: "ENOTSUP".into(),
        },
        Msg::PlatformOperationFailed {
            operation: "fsync".into(),
            reason: "EIO".into(),
        },
        Msg::WorkspaceOpenFailed {
            root: "/p".into(),
            reason: "no such file".into(),
        },
        Msg::PathNotARegularFile { path: "src".into() },
        Msg::PathComponentNotADirectory {
            path: "a/b".into(),
            component: "a".into(),
        },
        Msg::PathSymlinkRefused {
            path: "src/link".into(),
        },
        Msg::PathParentMissing {
            path: "a/b/c.rs".into(),
            parent: "a/b".into(),
        },
        Msg::HintCreateTheDirectoryFirst,
        Msg::ScopeNotCovered {
            path: "src/a.rs".into(),
        },
        Msg::ScopeDeniedByRule {
            path: "a/.env".into(),
            pattern: "**/.env".into(),
        },
        Msg::ScopeOperationNotAllowed {
            path: "src/a.rs".into(),
            operation: "move".into(),
            allowed: "create, replace".into(),
        },
        Msg::HintExpansionMayBeRequested,
        Msg::HintPolicyDenyIsFinal,
        Msg::PolicyNoAllowRules,
        Msg::PolicyEmptyDefaultOps,
        Msg::PolicyRuleGrantsNothing {
            pattern: "src/**".into(),
        },
        Msg::PolicyAllowOverProtected {
            pattern: ".git/**".into(),
            protected: ".git/**".into(),
        },
        Msg::PolicyWorkspaceWideNeedsOptIn {
            pattern: "**".into(),
        },
        Msg::PolicyAllowAlsoDenied {
            pattern: "src/**".into(),
        },
        Msg::PolicyDuplicatePattern {
            pattern: "src/**".into(),
        },
        Msg::PolicyDenyNeverApplies {
            pattern: "docs/**".into(),
        },
        Msg::PolicyBudgetZero {
            field: "max_operations".into(),
        },
        Msg::PolicyFileLimitExceedsSnapshotLimit {
            file_bytes: 2048,
            snapshot_bytes: 1024,
        },
        Msg::PolicyWarnRatioOutOfRange { value: 1.5 },
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
    // The sample list is maintained by hand, so a variant added without a sample
    // would silently shrink this test's reach. `std::mem::variant_count` is not
    // stable, so the count is pinned instead: adding a variant means adding a
    // sample and bumping this number, which is a visible edit rather than an
    // omission.
    assert_eq!(
        samples.len(),
        79,
        "add the new Msg variant to this list, then update the count"
    );

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
fn every_label_renders_in_every_language() {
    // Labels are a nested enum, so the sample list above covers only one of
    // them; each language matches exhaustively, and this checks the results are
    // non-empty.
    let labels = [
        Label::Allowed,
        Label::Refused,
        Label::NotCovered,
        Label::Path,
        Label::Operation,
        Label::Policy,
        Label::PolicyVersion,
        Label::EvaluationSteps,
        Label::StepProtected,
        Label::StepDeny,
        Label::StepAllow,
        Label::StepGrant,
        Label::NoMatch,
        Label::Outcome,
        Label::ExpansionPossible,
        Label::ExpansionImpossible,
        Label::CurrentScope,
        Label::Nothing,
        Label::Warnings,
        Label::Approved,
        Label::UnapprovedEdits,
    ];
    for label in labels {
        for language in Language::ALL {
            let text = Msg::Label(label).render(language);
            assert!(
                !text.trim().is_empty(),
                "{label:?} has no {language} translation"
            );
        }
    }
}
