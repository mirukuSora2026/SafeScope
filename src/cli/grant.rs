//! Approving a scope expansion at a terminal.
//!
//! This is the path that does not depend on a client being honest. A person runs
//! it, sees exactly which paths are being opened, and types the word.
//!
//! It deliberately does not take the workspace lock. An approval is not a file
//! change, and the situation it exists for is a server already running and
//! holding that lock — requiring it would make the command useless precisely
//! when it is needed.
//!
//! As with `policy approve`, the terminal check is not a strong boundary and is
//! not presented as one. An agent with shell access can run this binary; what it
//! cannot easily do is answer a prompt on a terminal it does not have.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::dataformatting::{Label, Msg};
use crate::domain::{OpSet, Operation};
use crate::error::{Denial, Error, ErrorCode, Result};
use crate::paths::RelPath;
use crate::policy::{ApprovalSource, Grant};
use crate::registry;
use crate::store::grant_store::GrantStore;
use crate::store::policy_store::PolicyStore;
use crate::store::task_store::TaskStore;

/// Issues a grant for exact paths, after asking the person to confirm.
pub fn run(workspace: &Path, paths: &[String], operations: &[String]) -> Result<()> {
    let registration = registry::load(workspace)?;
    let state = registration.state_paths()?;

    let approved = PolicyStore::new(&state).current()?.ok_or_else(|| {
        Error::Denied(
            Denial::new(ErrorCode::NoApprovedPolicy, Msg::Label(Label::Nothing)).with_hint(
                Msg::HintFillInAllowThenApprove {
                    policy: registration.policy_path().display().to_string(),
                },
            ),
        )
    })?;

    // Exact paths, never patterns. Asked for one file, a client could otherwise
    // propose a directory as "the pattern for it".
    let paths = paths
        .iter()
        .map(|text| RelPath::parse(text))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let operations = parse_operations(operations)?;

    if !std::io::stdin().is_terminal() {
        return Err(Error::Denied(
            Denial::new(ErrorCode::ApprovalNeedsTty, Msg::ExpansionNeedsTerminal).with_hint(
                Msg::HintApproveAtATerminal {
                    paths: describe(&paths),
                },
            ),
        ));
    }

    let minutes = approved.policy.approval.grant_ttl_minutes;
    summarise(&paths, operations, minutes);
    if !confirmed(&mut std::io::stdin().lock(), &mut std::io::stdout())? {
        return Err(Error::Denied(Denial::new(
            ErrorCode::ApprovalRequired,
            Msg::ExpansionDeclined,
        )));
    }

    // A task must already exist. Starting one here would silently reset a
    // budget, which is not what somebody typing "approve" asked for.
    let task = TaskStore::new(&state).current_or_start()?;
    let grant = Grant::new(
        task,
        approved.version,
        paths.clone(),
        operations,
        ApprovalSource::Terminal,
        SystemTime::now() + Duration::from_secs(minutes * 60),
    );
    GrantStore::new(&state).issue(&grant)?;

    println!(
        "{}",
        Msg::ExpansionGranted {
            paths: paths.len(),
            minutes
        }
    );
    Ok(())
}

fn parse_operations(names: &[String]) -> Result<OpSet> {
    if names.is_empty() {
        return Ok(OpSet::all());
    }
    names
        .iter()
        .map(|name| {
            name.parse::<Operation>()
                .map_err(|reason| Error::Denied(Denial::new(ErrorCode::InvalidPath, reason)))
        })
        .collect::<Result<Vec<_>>>()
        .map(|operations| operations.into_iter().collect())
}

fn summarise(paths: &[RelPath], operations: OpSet, minutes: u64) {
    println!(
        "{}",
        Msg::ExpansionPrompt {
            paths: describe(paths),
            operations: operations.to_string(),
            reason: Msg::Label(Label::Nothing).to_string(),
        }
    );
    println!(
        "{}",
        Msg::ExpansionGranted {
            paths: paths.len(),
            minutes
        }
    );
}

fn describe(paths: &[RelPath]) -> String {
    paths
        .iter()
        .map(|path| path.as_str().to_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Asks the person to type the word, so a stray keypress cannot open a path.
fn confirmed(input: &mut impl BufRead, output: &mut impl Write) -> Result<bool> {
    write!(output, "Type `approve` to confirm: ").map_err(Error::from)?;
    output.flush().map_err(Error::from)?;

    let mut answer = String::new();
    input.read_line(&mut answer).map_err(Error::from)?;
    Ok(answer.trim() == "approve")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_whole_word_confirms() {
        for answer in ["approve\n", "  approve  \n"] {
            assert!(confirmed(&mut answer.as_bytes(), &mut Vec::new()).unwrap());
        }
        for answer in ["\n", "y\n", "yes\n", "APPROVE\n", ""] {
            assert!(
                !confirmed(&mut answer.as_bytes(), &mut Vec::new()).unwrap(),
                "{answer:?} should not confirm"
            );
        }
    }

    #[test]
    fn an_empty_operation_list_grants_every_operation() {
        assert_eq!(parse_operations(&[]).unwrap(), OpSet::all());
    }

    #[test]
    fn operations_are_named_not_guessed() {
        let set = parse_operations(&["replace".to_owned(), "trash".to_owned()]).unwrap();
        assert!(set.contains(Operation::Replace));
        assert!(!set.contains(Operation::Create));
        assert!(parse_operations(&["delete".to_owned()]).is_err());
    }
}
