//! The `safescope` command line.
//!
//! Three commands in this version. `hook` and the MCP server arrive with M4,
//! once their wire formats have been read from the host's documentation rather
//! than guessed at.

pub mod approve;
pub mod check;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::dataformatting::Msg;
use crate::error::{Error, Result};
use crate::registry;

/// Process exit codes.
///
/// A refusal and a failure are distinguished here as they are everywhere else,
/// so a script can tell "the policy said no" from "the engine broke".
pub mod exit {
    pub const OK: i32 = 0;
    pub const DENIED: i32 = 1;
    pub const FAILED: i32 = 2;
}

#[derive(Debug, Parser)]
#[command(
    name = "safescope",
    version,
    about = "Scope, budget and undo for file changes"
)]
pub struct Cli {
    /// The project to act on.
    #[arg(long, global = true, default_value = ".", value_name = "DIR")]
    pub workspace: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Register this project as a workspace.
    Init,
    /// Inspect and approve the policy.
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },
    /// Explain whether an operation on a path would be permitted.
    ///
    /// Changes nothing. This is the command to reach for when a refusal is
    /// puzzling.
    Check {
        /// Path relative to the workspace root.
        path: String,
        /// One of create, replace, move, trash.
        #[arg(long, value_name = "OPERATION")]
        op: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum PolicyAction {
    /// Approve the current policy file, after checking it.
    ///
    /// Must be run by a person at a terminal.
    Approve,
    /// Show the approved policy and whether the file has since been edited.
    Show,
}

/// Runs a parsed command, returning the process exit code.
pub fn run(cli: Cli) -> i32 {
    match dispatch(&cli) {
        Ok(code) => code,
        Err(error) => {
            let report = error.report();
            eprintln!("{}: {}", report.code, report.message);
            if let Some(hint) = report.hint {
                eprintln!("{hint}");
            }
            if error.is_denial() {
                exit::DENIED
            } else {
                exit::FAILED
            }
        }
    }
}

/// Runs the command, returning the exit code it wants.
///
/// `check` explains a refusal in full and then reports it through the exit code,
/// so the generic error printer does not repeat what the command already said.
fn dispatch(cli: &Cli) -> Result<i32> {
    match &cli.command {
        Command::Init => init(cli).map(|()| exit::OK),
        Command::Policy {
            action: PolicyAction::Approve,
        } => approve::run(&cli.workspace).map(|()| exit::OK),
        Command::Policy {
            action: PolicyAction::Show,
        } => approve::show(&cli.workspace).map(|()| exit::OK),
        Command::Check { path, op } => check::run(&cli.workspace, path, op),
    }
}

fn init(cli: &Cli) -> Result<()> {
    let registration = registry::init(&cli.workspace)?;
    println!(
        "{}",
        Msg::WorkspaceRegistered {
            root: registration.root().display().to_string(),
            id: registration.id().to_string(),
        }
    );
    println!(
        "{}",
        Msg::HintFillInAllowThenApprove {
            policy: registration.policy_path().display().to_string(),
        }
    );
    Ok(())
}

/// Reports an error the same way [`run`] does, for callers that catch one early.
pub fn report(error: &Error) {
    let report = error.report();
    eprintln!("{}: {}", report.code, report.message);
}
