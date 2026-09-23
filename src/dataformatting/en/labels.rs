//! The en spellings of the fixed labels.
//!
//! Split from the messages beside them only for length. It is still one
//! exhaustive match on `Label`, so a new label still fails to compile until
//! every language has one.

use crate::dataformatting::Label;

pub(super) fn render(label: Label) -> &'static str {
    match label {
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
}
