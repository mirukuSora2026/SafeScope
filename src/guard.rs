//! Running a command that cannot write to the workspace.
//!
//! Everything else in this crate is a check that something has to agree to
//! consult. A policy is checked when a change comes through the engine; a hook
//! is consulted when the host chooses to run it; an allowlist refuses tools the
//! host offers it the chance to refuse. All of that was measured to be worth
//! having and none of it is a boundary: a hook can be turned off, and a tool the
//! host never shows the hook is a tool the hook never sees.
//!
//! This is the boundary. The command runs under a kernel sandbox that denies
//! every write to the workspace, so a change that did not go through the engine
//! is not refused — it is impossible. The engine runs out here, outside the
//! sandbox, and the only thing inside is a relay that carries bytes to it.
//!
//! ```text
//!   safescope guard -- claude …
//!     ├── the engine, listening on a socket beside the workspace's state
//!     └── sandbox-exec: claude, and everything it starts
//!            └── safescope mcp  →  relays to the socket; writes nothing
//! ```
//!
//! **macOS only.** The sandbox is the whole of what this offers, so where there
//! is none it refuses rather than running the command unprotected — a guard that
//! sometimes guards is worse than one that says it cannot.
//!
//! It denies writes without exception, including caches and temporary files. A
//! test run that wants to write `__pycache__` will find it cannot. That is the
//! cost of the guarantee and it is not softened here: an exception is a path
//! where an unrecorded change can happen, which is the thing being removed.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::dataformatting::Msg;
use crate::error::{Denial, Error, ErrorCode, Fault, Result};
use crate::mcp::socket::{self, SOCKET_ENV};
use crate::registry;

/// The sandbox profile: everything as usual, except writing to the workspace.
///
/// `(allow default)` rather than an allowlist of operations. This is not trying
/// to contain a hostile program — it is removing one capability from a trusted
/// one, and a profile that also broke its network access or its temporary files
/// would be a profile people turn off.
fn profile(workspace: &Path) -> String {
    // Both the path as given and the path the kernel sees. On macOS `/var` is a
    // symlink to `/private/var`, so a profile naming only the first denies a
    // subtree nothing is ever resolved into — it loads, it reports no error, and
    // it protects nothing. That failure is silent, which is the worst kind here.
    let mut subtrees = vec![workspace.to_path_buf()];
    if let Ok(resolved) = workspace.canonicalize()
        && resolved != workspace
    {
        subtrees.push(resolved);
    }
    let denied = subtrees
        .iter()
        .map(|path| format!("   (subpath {})\n", quote(&path.to_string_lossy())))
        .collect::<String>();

    format!(
        "(version 1)\n\
         (allow default)\n\
         (deny file-write*\n{denied})\n"
    )
}

/// A seatbelt string literal.
fn quote(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn unsupported() -> Error {
    Error::Denied(
        Denial::new(ErrorCode::UnsupportedOperation, Msg::GuardUnsupportedHere)
            .with_hint(Msg::HintGuardNeedsSandbox),
    )
}

/// Runs `command` with the workspace made unwritable to it.
///
/// Returns the command's own exit code, so this can stand in front of anything
/// without changing what a script sees.
pub fn run(workspace: &Path, command: &[String]) -> Result<i32> {
    if !cfg!(target_os = "macos") {
        return Err(unsupported());
    }
    // The command is `required` at the command line, so an empty one cannot
    // arrive here from a person; this keeps a caller in-process honest.
    let Some((program, arguments)) = command.split_first() else {
        return Err(unsupported());
    };

    let registration = registry::load(workspace)?;
    let paths = registration.state_paths()?;
    paths.create()?;
    let root = registration.root().to_path_buf();

    // Outside the workspace, or it would be unreachable under the very rule
    // this sets up — and short, because a socket path has a length limit the
    // state directory alone can overrun.
    let socket_path = socket::path_for(registration.id())?;

    let policy_path = paths.root().join("guard.sb");
    std::fs::write(&policy_path, profile(&root))
        .map_err(|error| Error::Faulted(Fault::io("could not write the sandbox profile", error)))?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| Error::Faulted(Fault::io("could not start the async runtime", error)))?;

    // Bound inside the runtime: a tokio listener registers with the reactor as
    // it is created, and there is not one until here.
    let listener = runtime.block_on(async { socket::bind(&socket_path) })?;

    let (stop, shutdown) = tokio::sync::oneshot::channel();
    let engine = runtime.spawn(socket::serve(root.clone(), listener, shutdown));

    println!(
        "{}",
        Msg::GuardStarting {
            path: root.display().to_string()
        }
    );

    let status = Command::new("sandbox-exec")
        .arg("-f")
        .arg(&policy_path)
        .arg(program)
        .args(arguments)
        .env(SOCKET_ENV, &socket_path)
        .current_dir(&root)
        .status();

    // Stopped before the result is examined, so a failure to start the command
    // does not leave an engine listening on a socket nobody will connect to.
    let _ = stop.send(());
    runtime.block_on(async {
        let _ = engine.await;
    });
    let _ = std::fs::remove_file(&socket_path);

    let status = status.map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => unsupported(),
        _ => Error::Faulted(Fault::io("could not start the guarded command", error)),
    })?;

    // A signalled child has no exit code. Reporting 0 would say it succeeded.
    Ok(status.code().unwrap_or(1))
}

/// Where the guard would put its socket, for a workspace.
pub fn socket_path(workspace: &Path) -> Result<PathBuf> {
    socket::path_for(registry::load(workspace)?.id())
}
