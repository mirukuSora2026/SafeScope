//! The screens a person reads.
//!
//! Three commands, all read-only, none of them taking the workspace lock: a
//! question that could not be asked while the MCP server was running would be a
//! question nobody could ask when it mattered.
//!
//! What they report is what happened, not a verdict on it. "3 / 8 paths" is
//! checkable against what somebody can see; "well within limits" is not.

use std::path::Path;

use crate::dataformatting::{self, Label, Msg, pad};
use crate::error::Result;
use crate::inspect::Inspector;
use crate::journal::OperationRecord;

/// Terminal cells reserved for a row label.
const LABEL_WIDTH: usize = 22;
/// Default number of history entries.
const DEFAULT_HISTORY: usize = 20;

/// Prints what the workspace can say about itself.
pub fn status(workspace: &Path) -> Result<i32> {
    let inspector = Inspector::open(workspace)?;
    let status = inspector.status()?;

    heading(Label::Task);
    match status.task {
        Some(task) => println!("  {task}"),
        None => println!("  {}", Msg::Label(Label::NotStarted)),
    }
    match status.policy_version {
        Some(version) => println!("  {} {version}", Msg::Label(Label::PolicyVersion)),
        None => println!("  {}", Msg::Label(Label::NoPolicyYet)),
    }

    println!();
    heading(Label::CurrentScope);
    if status.allowed.is_empty() {
        println!("  {}", Msg::Label(Label::Nothing));
    }
    for entry in &status.allowed {
        println!("  {}  ({})", entry.pattern, entry.ops);
    }

    if let Some(limits) = status.limits {
        println!();
        heading(Label::Usage);
        row(
            Label::ChangedPaths,
            &dataformatting::usage(status.usage.paths(), limits.max_changed_paths),
        );
        row(
            Label::Operations,
            &dataformatting::usage(status.usage.operations, limits.max_operations),
        );
        row(
            Label::Moves,
            &dataformatting::usage(status.usage.moves, limits.max_moves),
        );
        row(
            Label::RecoveryStorage,
            &format!(
                "{} / {}",
                dataformatting::bytes(status.usage.snapshot_bytes),
                dataformatting::bytes(limits.max_snapshot_bytes)
            ),
        );
    }

    println!();
    heading(Label::State);
    row(Label::Unfinished, &count_or_none(status.unsettled));
    row(
        Label::NeedsComparing,
        &count_or_none(status.needs_attention),
    );
    row(Label::TemporaryApprovals, &count_or_none(status.grants));
    if status.unapproved_policy_edits {
        println!("  {}", Msg::Label(Label::UnapprovedEdits));
    }

    if let Some(record) = &status.last_operation {
        println!();
        heading(Label::LastChange);
        println!("  {}", describe(record));
    }

    println!();
    heading(Label::Coverage);
    println!("  {}", Msg::McpCoverageNotice);

    // An unsettled workspace is not a healthy one, and the exit code says so.
    Ok(if status.is_settled() {
        crate::cli::exit::OK
    } else {
        crate::cli::exit::DENIED
    })
}

/// Prints what the current task has done.
pub fn history(workspace: &Path, limit: Option<usize>) -> Result<i32> {
    let inspector = Inspector::open(workspace)?;
    let entries = inspector.history(limit.unwrap_or(DEFAULT_HISTORY))?;

    heading(Label::History);
    if entries.is_empty() {
        println!("  {}", Msg::Label(Label::Nothing));
        return Ok(crate::cli::exit::OK);
    }
    for record in &entries {
        println!("  {}", describe(record));
    }
    Ok(crate::cli::exit::OK)
}

/// Runs a handful of checks and says which ones hold.
///
/// Reports each check by name whether it passed or not. Listing only the
/// failures would read as a clean bill of health for everything it never looked
/// at.
pub fn doctor(workspace: &Path) -> Result<i32> {
    let inspector = Inspector::open(workspace)?;
    let status = inspector.status()?;

    heading(Label::Checks);
    // Each check is named whether it passed or not. Listing only the failures
    // would read as a clean bill of health for everything it never looked at.
    let checks = [
        (Label::Policy, status.policy_version.is_some()),
        (Label::Task, status.task.is_some()),
        (Label::Unfinished, status.unsettled == 0),
        (Label::NeedsComparing, status.needs_attention == 0),
        (Label::PolicyFile, !status.unapproved_policy_edits),
    ];

    let mut healthy = true;
    for (label, passed) in checks {
        let outcome = if passed { Label::Passed } else { Label::Failed };
        println!(
            "  {}{}",
            pad(&Msg::Label(label).to_string(), LABEL_WIDTH),
            Msg::Label(outcome)
        );
        healthy &= passed;
    }

    println!();
    if healthy {
        println!("{}", Msg::DoctorHealthy);
    }
    for note in inspector.concerns()? {
        println!("{note}");
    }

    Ok(if healthy {
        crate::cli::exit::OK
    } else {
        crate::cli::exit::DENIED
    })
}

fn heading(label: Label) {
    println!("{}", Msg::Label(label));
}

fn row(label: Label, value: &str) {
    println!(
        "  {}{value}",
        pad(&Msg::Label(label).to_string(), LABEL_WIDTH)
    );
}

fn count_or_none(count: usize) -> String {
    if count == 0 {
        Msg::Label(Label::Nothing).to_string()
    } else {
        count.to_string()
    }
}

/// One operation, on one line.
fn describe(record: &OperationRecord) -> String {
    let paths = record
        .transition
        .touched_paths()
        .into_iter()
        .map(|path| path.as_str().to_owned())
        .collect::<Vec<_>>()
        .join(" -> ");

    let mut line = format!(
        "{}{}{}",
        pad(&record.sequence.to_string(), 4),
        pad(record.transition.operation().as_str(), 9),
        paths
    );
    // Only worth saying when it is not the ordinary case.
    if record.kind != crate::journal::OperationKind::Change {
        line.push_str(&format!("  [{}]", record.kind));
    }
    if record.stage != crate::journal::Stage::Committed {
        line.push_str(&format!("  [{}]", record.stage));
    }
    line
}
