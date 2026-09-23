//! English messages.

use crate::dataformatting::{Label, Msg};

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
        Msg::PolicyParseFailed { reason } => {
            format!("The policy file could not be read: {reason}")
        }
        Msg::PolicySchemaUnsupported { found, supported } => format!(
            "Unsupported policy schema_version {found}; this build understands {supported}."
        ),
        Msg::Label(label) => match label {
            Label::Allowed => "Allowed",
            Label::Refused => "Refused",
            Label::NotCovered => "Not covered",
            Label::Path => "Path",
            Label::Operation => "Operation",
            Label::Policy => "Policy",
            Label::PolicyVersion => "Version",
            Label::EvaluationSteps => "Evaluation",
            Label::StepProtected => "protected paths",
            Label::StepDeny => "deny rules",
            Label::StepAllow => "allow rules",
            Label::StepGrant => "temporary approvals",
            Label::NoMatch => "no match",
            Label::Outcome => "Outcome",
            Label::ExpansionPossible => "a scope expansion may be requested",
            Label::ExpansionImpossible => "this cannot be opened by an approval",
            Label::CurrentScope => "Allowed scope",
            Label::Nothing => "nothing",
            Label::Warnings => "Warnings",
            Label::Approved => "Approved",
            Label::UnapprovedEdits => "the policy file has unapproved edits",
            Label::Task => "Task",
            Label::Usage => "Usage",
            Label::State => "State",
            Label::LastChange => "Last change",
            Label::ChangedPaths => "changed paths",
            Label::Operations => "operations",
            Label::Moves => "moves",
            Label::RecoveryStorage => "recovery storage",
            Label::Unfinished => "unfinished",
            Label::NeedsComparing => "needs comparing",
            Label::TemporaryApprovals => "temporary approvals",
            Label::PolicyFile => "policy file",
            Label::History => "History",
            Label::Coverage => "Coverage",
            Label::NotStarted => "no task started",
            Label::NoPolicyYet => "no policy approved",
            Label::Checks => "Checks",
            Label::Passed => "ok",
            Label::Failed => "FAILED",
            Label::ChangedOutside => "Changed outside SafeScope",
            Label::NoBaseline => "no baseline",
            Label::DriftAdded => "added",
            Label::DriftModified => "changed",
            Label::DriftRemoved => "removed",
        }
        .to_owned(),
        Msg::WorkspaceAlreadyRegistered { root } => {
            format!("{root} is already a SafeScope workspace.")
        }
        Msg::WorkspaceNotRegistered { root } => {
            format!("{root} is not a SafeScope workspace.")
        }
        Msg::WorkspaceIdCorrupted { path } => {
            format!("The workspace identity at {path} is not readable.")
        }
        Msg::WorkspaceRegistered { root, id } => {
            format!("Registered {root} as workspace {id}.")
        }
        Msg::WorkspaceBusyElsewhere { path } => format!(
            "Another SafeScope process is writing to this workspace (its lock is at {path}). \
             Only one writer at a time, so a budget check and the reservation it leads to \
             cannot be interleaved with somebody else's."
        ),
        Msg::HintAnotherSessionIsWriting => {
            "Wait for the other session to finish, or close it.".to_owned()
        }
        Msg::HintRunInitFirst => "Run `safescope init` in the project first.".to_owned(),
        Msg::HintFillInAllowThenApprove { policy } => format!(
            "Nothing can be changed yet. Add the paths you want to allow to {policy}, then run \
             `safescope policy approve`."
        ),
        Msg::ExpansionPrompt {
            paths,
            operations,
            reason,
        } => format!(
            "SafeScope is being asked to widen what it may change.\n\n\
             Paths: {paths}\n\
             Operations: {operations}\n\
             Reason given: {reason}\n\n\
             Approving opens exactly these paths, for this task, for a limited time. It does \
             not change the policy."
        ),
        Msg::ExpansionAlreadyAllowed { path } => {
            format!("{path} is already within the allowed scope; nothing needs approving.")
        }
        Msg::ExpansionCannotBeGranted { path } => format!(
            "{path} cannot be opened by an approval. A protected path or a deny rule is not \
             a matter of permission, so asking would only waste somebody's attention."
        ),
        Msg::ExpansionNeedsTerminal => "This client cannot put a question to a person, so no \
approval can be obtained through it."
            .to_owned(),
        Msg::ExpansionLimitReached { used, limit } => format!(
            "This task has already obtained {used} of {limit} approvals through the client. \
             Further ones have to be given at a terminal, so a long run of requests cannot \
             quietly become a wide one."
        ),
        Msg::ExpansionDeclined => "The request was declined. Nothing was opened.".to_owned(),
        Msg::ExpansionGranted { paths, minutes } => {
            format!("Approved: {paths} path(s), for this task, for {minutes} minutes.")
        }
        Msg::HintApproveAtATerminal { paths } => {
            format!("Run `safescope approve {paths}` in a terminal to grant this yourself.")
        }
        Msg::McpInstructions => "SafeScope keeps file changes inside an approved scope and \
budget, and records them so they can be reviewed and undone.\n\n\
Call prepare_change to check a change and stage it; it alters nothing. Then call \
apply_change with the plan id it returned. A refusal names the rule responsible and says \
whether asking for a wider scope could change the answer — when it says it cannot, do not \
retry the same request.\n\n\
SafeScope only knows about changes made through these tools. Anything written by a shell \
command is not recorded and cannot be undone here."
            .to_owned(),
        Msg::McpCoverageNotice => "Only changes made through SafeScope are recorded. Files \
written by a shell command or another tool are not covered and cannot be undone here."
            .to_owned(),
        Msg::McpMissingField { field } => {
            format!("This operation needs a {field} and none was given.")
        }
        Msg::McpUnknownPlan { plan } => format!(
            "There is no prepared plan {plan}. It may have been applied already, or the \
             server may have restarted since."
        ),
        Msg::McpTransportFailed { reason } => {
            format!("The MCP connection could not be served: {reason}")
        }
        Msg::HookUnsettledWork { count } => format!(
            "SafeScope has {count} operation(s) it did not finish recording. Run \
             `safescope recover` before relying on what is on disk."
        ),
        Msg::HookNeedsAttention { count } => format!(
            "SafeScope cannot say what became of {count} operation(s); somebody has to \
             compare them. Nothing was repaired automatically."
        ),
        Msg::HookPolicyEdited => "The SafeScope policy file has been edited but not approved, \
so the change is not in force. Run `safescope policy approve` if it was meant to be."
            .to_owned(),
        Msg::HookChangedOutside { count } => format!(
            "{count} file(s) changed without going through SafeScope, so they have no \
             snapshot and cannot be undone. Run `safescope status` to see which."
        ),
        Msg::HookToolNotAllowed { tool } => format!(
            "The approved policy runs SafeScope in allowlist mode, and `{tool}` is not on \
             the list. Change files through SafeScope so the change is recorded."
        ),
        Msg::DriftNoBaseline => {
            "No baseline has been taken, so SafeScope cannot say what changed around it.".to_owned()
        }
        Msg::DriftClean { scanned } => {
            format!("Nothing changed outside SafeScope ({scanned} file(s) checked).")
        }
        Msg::DriftUnrecoverable { count } => format!(
            "{count} of these have no stored previous contents. SafeScope cannot undo them."
        ),
        Msg::DriftTruncated { scanned } => {
            format!("Only the first {scanned} files were checked, so this list may be incomplete.")
        }
        Msg::HintReviewDrift => {
            "Review these, then run `safescope drift accept` to adopt them as the new baseline."
                .to_owned()
        }
        Msg::HintAllowlistMode { allowed } => format!("Tools the policy allows: {allowed}"),
        Msg::GuardUnsupportedHere => {
            "A guarded run needs a kernel sandbox, and this platform has none.".to_owned()
        }
        Msg::HintGuardNeedsSandbox => {
            "`safescope guard` works on macOS, through sandbox-exec. Elsewhere, run the \
command without it and use `safescope drift` to see what changed around the engine."
                .to_owned()
        }
        Msg::GuardStarting { path } => format!(
            "Guarding {path}. Nothing this command starts can write there; changes have to \
             go through SafeScope."
        ),
        Msg::DurabilityLimitedHere => {
            "This platform cannot flush a directory, so a crash immediately after a change \
can lose that change. It cannot leave a half-written file, and `safescope recover` \
reports anything it finds."
                .to_owned()
        }
        Msg::PolicyAllowToolsWithoutAllowlist => {
            "`allow_tools` is set but the mode is `audit`, where nothing is refused by \
name — so the list has no effect. Set `mode = \"allowlist\"` if it was meant to."
                .to_owned()
        }
        Msg::PolicyAllowToolsReopensTheGap { tool } => format!(
            "`allow_tools` names `{tool}`, which can run a shell command. Allowing it \
             reopens the route the allowlist exists to close; changes made that way are \
             not recorded."
        ),
        Msg::RecoveryFoundNothing => "Nothing needed recovering.".to_owned(),
        Msg::RecoverySettled { aborted, committed } => {
            format!("Settled {committed} operation(s) that had run and {aborted} that had not.")
        }
        Msg::RecoveryLeftUnresolved { count } => format!(
            "{count} operation(s) could not be settled: what is on disk matches neither what \
             was there before nor what the change would have produced. Nothing was repaired — \
             guessing here is how intact work gets destroyed."
        ),
        Msg::RecoveryRemovedTemporaries { count } => {
            format!("Removed {count} temporary file(s) a crash left behind.")
        }
        Msg::DoctorHealthy => "Everything checked here is in order.".to_owned(),
        Msg::UndoNothingRecorded => "There is nothing to undo: this task has no completed \
operation. An operation whose outcome is still unclear has to be settled first."
            .to_owned(),
        Msg::UndoConflictAt { path } => format!(
            "{path} has changed since SafeScope last touched it, so undoing would overwrite \
             whatever arrived afterwards. Nothing was done."
        ),
        Msg::HintCompareBeforeUndoing { path } => format!(
            "Compare {path} against the stored previous contents and decide what should \
             survive; the recovery data is kept either way."
        ),
        Msg::BudgetPathsExceeded {
            used,
            limit,
            adding,
        } => format!(
            "This task has already changed {used} of {limit} permitted paths, and {adding} \
             would be another. Nothing was done."
        ),
        Msg::BudgetOperationsExceeded { used, limit } => {
            format!("This task has used {used} of {limit} permitted operations.")
        }
        Msg::BudgetMovesExceeded { used, limit } => {
            format!("This task has used {used} of {limit} permitted moves.")
        }
        Msg::BudgetStorageExceeded { used, limit } => format!(
            "Recovery data for this workspace is {used} against a {limit} limit, so the \
             contents this change would destroy could not be kept."
        ),
        Msg::HintRequestBudgetExpansion => "Review what has been changed so far. If the work \
genuinely needs more, ask for the limit to be raised rather than retrying."
            .to_owned(),
        Msg::PlanTargetExists { path } => format!(
            "{path} already exists, so it cannot be created. Replacing it is a different \
             operation, and one that stores the previous contents first."
        ),
        Msg::PlanTargetMissing { path } => {
            format!("{path} does not exist, so this operation has nothing to act on.")
        }
        Msg::PlanFileTooLarge { path, size, limit } => {
            format!("{path} is {size}, over the {limit} this policy permits.")
        }
        Msg::PlanHasExpired { plan } => {
            format!("Plan {plan} has expired and will not be applied.")
        }
        Msg::PlanStateChanged { path } => format!(
            "{path} has changed since this was planned. Nothing was done: applying a plan \
             built against different contents would overwrite whatever arrived in between."
        ),
        Msg::HintRebuildThePlan => "Build the plan again against the current contents.".to_owned(),
        Msg::JournalOpenFailed { path, reason } => {
            format!("The journal at {path} could not be opened: {reason}")
        }
        Msg::JournalOperationFailed { reason } => {
            format!("The journal could not be written: {reason}")
        }
        Msg::JournalRequestMismatch { request } => format!(
            "Request {request} was already recorded with different contents. It is reported \
             rather than treated as a new request, because guessing which one was meant could \
             apply a change twice."
        ),
        Msg::JournalUnknownOperation { operation } => {
            format!("There is no journal record for operation {operation}.")
        }
        Msg::SnapshotVerificationFailed { hash } => format!(
            "The recovery data for {hash} does not hash back to what was stored. Nothing was \
             changed: without a snapshot that reads back correctly, the change could not be \
             undone."
        ),
        Msg::SnapshotMissing { hash } => {
            format!("The recovery data for {hash} is not in the store.")
        }
        Msg::SnapshotStoreFailed { reason } => {
            format!("Recovery data could not be stored: {reason}. The original was left untouched.")
        }
        Msg::StoreDataDirectoryUnavailable => "No directory is available for engine state. \
Set SAFESCOPE_DATA_DIR to choose one."
            .to_owned(),
        Msg::StoreWriteFailed { path, reason } => {
            format!("Could not write engine state to {path}: {reason}")
        }
        Msg::StoreReadFailed { path, reason } => {
            format!("Could not read engine state from {path}: {reason}")
        }
        Msg::StoreCorrupted { path, reason } => format!(
            "Engine state at {path} is not readable: {reason}. It was not repaired \
             automatically, because guessing at damaged state is how good data is lost."
        ),
        Msg::StoreStateInsideWorkspace { state, workspace } => format!(
            "Engine state would live at {state}, inside the workspace {workspace}. Snapshots \
             would then sit inside the tree they protect, so registration is refused."
        ),
        Msg::PathDestinationExists { path } => format!(
            "{path} already exists. A move never overwrites its destination, because the \
             file that would be lost has no snapshot."
        ),
        Msg::PlatformCrossFilesystem { from, to } => format!(
            "{from} and {to} are on different filesystems. A move between them cannot be \
             atomic, so it is refused rather than performed as a copy and a delete."
        ),
        Msg::PlatformAtomicRenameUnsupported { reason } => format!(
            "This filesystem cannot rename without overwriting ({reason}). Checking first and \
             renaming after would reopen the very race the operation exists to close, so the \
             operation is refused instead."
        ),
        Msg::PlatformOperationFailed { operation, reason } => {
            format!("The {operation} operation failed: {reason}")
        }
        Msg::WorkspaceOpenFailed { root, reason } => {
            format!("The workspace at {root} could not be opened: {reason}")
        }
        Msg::PathNotARegularFile { path } => {
            format!("{path} is not a regular file. This version handles single regular files only.")
        }
        Msg::PathComponentNotADirectory { path, component } => {
            format!("While resolving {path}, {component:?} is not a directory.")
        }
        Msg::PathSymlinkRefused { path } => format!(
            "{path} is a symbolic link. Following one would move the operation outside the \
             path that was checked, so it is refused rather than resolved."
        ),
        Msg::PathParentMissing { path, parent } => {
            format!("{path} cannot be created because the directory {parent} does not exist.")
        }
        Msg::HintCreateTheDirectoryFirst => "This version does not create directories. Create \
it yourself, then retry."
            .to_owned(),
        Msg::ScopeNotCovered { path } => {
            format!("{path} matches no rule in the allowed scope.")
        }
        Msg::ScopeDeniedByRule { path, pattern } => {
            format!("{path} is refused by the deny rule {pattern:?}.")
        }
        Msg::ScopeOperationNotAllowed {
            path,
            operation,
            allowed,
        } => format!("{path} may be changed, but not by {operation}. Allowed here: {allowed}."),
        Msg::HintExpansionMayBeRequested => "If this path really is needed, ask for a scope \
expansion through safescope rather than retrying the same edit."
            .to_owned(),
        Msg::HintPolicyDenyIsFinal => "A deny rule cannot be lifted by an approval. Change the \
policy file and approve it if this is wrong."
            .to_owned(),
        Msg::PolicyNoAllowRules => "The policy has no allow rules, so nothing could ever \
be changed. If that is intended, say so explicitly rather than leaving the list empty."
            .to_owned(),
        Msg::PolicyEmptyDefaultOps => "default_ops is empty, so shorthand allow rules would \
grant no operations."
            .to_owned(),
        Msg::PolicyRuleGrantsNothing { pattern } => {
            format!("The rule for {pattern:?} grants no operations.")
        }
        Msg::PolicyAllowOverProtected { pattern, protected } => format!(
            "The allow rule {pattern:?} reaches {protected:?}, which the engine always \
             protects, so the rule can never take effect."
        ),
        Msg::PolicyWorkspaceWideNeedsOptIn { pattern } => format!(
            "The allow rule {pattern:?} covers the whole workspace. Set \
             safety.unsafe_allow_workspace_wide = true if that is intended."
        ),
        Msg::PolicyAllowAlsoDenied { pattern } => format!(
            "{pattern:?} appears in both allow and deny. Deny always wins, so the intent \
             is unclear."
        ),
        Msg::PolicyDuplicatePattern { pattern } => {
            format!("The pattern {pattern:?} is listed more than once.")
        }
        Msg::PolicyDenyNeverApplies { pattern } => format!(
            "The deny rule {pattern:?} does not overlap any allow rule, so it has no effect."
        ),
        Msg::PolicyBudgetZero { field } => {
            format!("budget.{field} is zero, which would refuse every operation.")
        }
        Msg::PolicyFileLimitExceedsSnapshotLimit {
            file_bytes,
            snapshot_bytes,
        } => format!(
            "max_file_bytes ({file_bytes}) exceeds max_snapshot_bytes ({snapshot_bytes}), so a \
             permitted file could never be snapshotted."
        ),
        Msg::PolicyWarnRatioOutOfRange { value } => {
            format!("budget.warn_at_ratio is {value}; it must be between 0 and 1.")
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
