//! The two commands that change something.
//!
//! Both open a [`WriteSession`] and therefore take the workspace lock. That is
//! right rather than unfortunate: reconciling the journal with the workspace
//! while a server was applying changes to it would be reconciling against a
//! moving target, and undoing underneath one would be worse.
//!
//! If the lock is held, they say so and stop. A person closing the other session
//! is a better outcome than a race.

use std::path::Path;

use crate::dataformatting::{Label, Msg};
use crate::error::Result;
use crate::journal::Stage;
use crate::session::WriteSession;

/// Reconciles the journal with what is on disk.
pub fn recover(workspace: &Path) -> Result<i32> {
    let mut session = WriteSession::open(workspace)?;
    let report = session.recover()?;

    if report.is_empty() {
        println!("{}", Msg::RecoveryFoundNothing);
        return Ok(crate::cli::exit::OK);
    }

    let committed = report.settled(Stage::Committed);
    let aborted = report.settled(Stage::Aborted);
    if committed + aborted > 0 {
        println!("{}", Msg::RecoverySettled { aborted, committed });
    }
    if !report.temporaries_removed.is_empty() {
        println!(
            "{}",
            Msg::RecoveryRemovedTemporaries {
                count: report.temporaries_removed.len()
            }
        );
    }

    let unresolved: Vec<_> = report.unresolved().collect();
    if unresolved.is_empty() {
        return Ok(crate::cli::exit::OK);
    }

    // Nothing was repaired. What the engine cannot conclude is put in front of a
    // person, because the cases it cannot decide are the ones where being wrong
    // destroys work.
    println!(
        "{}",
        Msg::RecoveryLeftUnresolved {
            count: unresolved.len()
        }
    );
    for conclusion in unresolved {
        println!("  {}  [{}]", conclusion.operation, conclusion.now);
    }
    Ok(crate::cli::exit::DENIED)
}

/// Reverses the most recent completed operation.
pub fn undo(workspace: &Path) -> Result<i32> {
    let mut session = WriteSession::open(workspace)?;
    let plan = session.prepare_undo()?;

    // Said before it happens, so a person watching sees which file moved and
    // which way.
    println!(
        "{}  {}",
        plan.plan.transition.operation(),
        plan.plan
            .transition
            .touched_paths()
            .into_iter()
            .map(|path| path.as_str().to_owned())
            .collect::<Vec<_>>()
            .join(" -> ")
    );

    let record = session.apply_undo(&plan)?;
    println!("{}: {}", Msg::Label(Label::Outcome), record.stage);
    Ok(crate::cli::exit::OK)
}
