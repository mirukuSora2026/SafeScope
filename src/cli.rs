//! The `safescope` command line.
//!
//! Three commands in this version. `hook` and the MCP server arrive with M4,
//! once their wire formats have been read from the host's documentation rather
//! than guessed at.

pub mod approve;
pub mod check;
pub mod grant;
pub mod hook;
pub mod repair;
pub mod report;

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
    /// Open exact paths for the current task, after confirming at a terminal.
    ///
    /// Grants exactly the paths named, for a limited time. It does not change
    /// the policy.
    Approve {
        /// Paths relative to the workspace root. Never patterns.
        #[arg(required = true, value_name = "PATH")]
        paths: Vec<String>,
        /// Which operations to allow. Defaults to all of them.
        #[arg(long = "op", value_name = "OPERATION")]
        operations: Vec<String>,
    },
    /// Report what this workspace has changed and what is left.
    ///
    /// Reads only, and takes no lock, so it works while the MCP server runs.
    Status,
    /// List what the current task has done.
    History {
        /// How many entries, most recent first.
        #[arg(long, value_name = "N")]
        limit: Option<usize>,
    },
    /// Check the workspace and report every check, not only the failures.
    Doctor,
    /// Work out what happened to operations the engine did not finish.
    ///
    /// Repairs nothing. What it cannot conclude is reported for a person.
    Recover,
    /// Run a command that cannot write to the workspace.
    ///
    /// The one thing here that is a boundary rather than a check. The command
    /// runs under a kernel sandbox with every write to the workspace denied, so
    /// a change outside the engine is impossible rather than refused. macOS
    /// only; elsewhere this refuses rather than running the command unguarded.
    Guard {
        /// The command to run, after `--`.
        #[arg(required = true, last = true, value_name = "COMMAND")]
        command: Vec<String>,
    },
    /// Report what changed without going through SafeScope.
    ///
    /// Detects; it does not prevent. A file listed here is already changed and
    /// its previous contents are already gone — the point is that it is named.
    Drift {
        #[command(subcommand)]
        action: Option<DriftAction>,
    },
    /// Reverse the most recent completed change.
    ///
    /// Refused if the file has been edited since, rather than overwriting that
    /// edit.
    Undo,
    /// Serve the MCP tools over stdio.
    ///
    /// Holds the workspace lock for as long as it runs, so a second server on
    /// the same workspace will not start.
    Mcp,
    /// Answer a Claude Code PreToolUse hook on stdin.
    ///
    /// Reads the pending tool call as JSON and writes a decision, or stays
    /// silent and lets the host's normal permission flow decide.
    Hook,
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

#[derive(Debug, Subcommand)]
pub enum DriftAction {
    /// Adopt the workspace as it stands as the new baseline.
    ///
    /// What a person does after reviewing drift and deciding to keep it.
    /// Anything outstanding stops being reported, so it is never done on the
    /// engine's own initiative.
    Accept,
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
        Command::Status => report::status(&cli.workspace),
        #[cfg(unix)]
        Command::Guard { command } => crate::guard::run(&cli.workspace, command),
        #[cfg(windows)]
        Command::Guard { .. } => Err(Error::Denied(
            crate::error::Denial::new(
                crate::error::ErrorCode::UnsupportedOperation,
                crate::dataformatting::Msg::GuardUnsupportedHere,
            )
            .with_hint(crate::dataformatting::Msg::HintGuardNeedsSandbox),
        )),
        Command::Drift { action } => match action {
            None => report::drift(&cli.workspace),
            Some(DriftAction::Accept) => repair::accept_drift(&cli.workspace),
        },
        Command::Recover => repair::recover(&cli.workspace),
        Command::Undo => repair::undo(&cli.workspace),
        Command::History { limit } => report::history(&cli.workspace, *limit),
        Command::Doctor => report::doctor(&cli.workspace),
        Command::Approve { paths, operations } => {
            grant::run(&cli.workspace, paths, operations).map(|()| exit::OK)
        }
        Command::Mcp => serve_mcp(&cli.workspace).map(|()| exit::OK),
        Command::Hook => hook::run(),
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

/// Runs the MCP server until the client disconnects.
///
/// The runtime is built here rather than around `main`, so every other command
/// stays synchronous and starts without one.
fn serve_mcp(workspace: &std::path::Path) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            Error::Faulted(crate::error::Fault::io(
                "could not start the async runtime",
                error,
            ))
        })?;
    runtime.block_on(crate::mcp::serve(workspace.to_path_buf()))
}

/// Reports an error the same way [`run`] does, for callers that catch one early.
pub fn report(error: &Error) {
    let report = error.report();
    eprintln!("{}: {}", report.code, report.message);
}
