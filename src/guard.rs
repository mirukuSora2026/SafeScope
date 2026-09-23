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
//! Two kernels can do this and they say it differently. macOS gets a seatbelt
//! profile denying writes under the workspace; Linux gets a Landlock ruleset
//! granting writes everywhere else, which is the same sentence in a language
//! with no "except" — see [`landlock`]. Anywhere else, and on a Linux too old
//! for Landlock, this refuses rather than running the command unprotected: a
//! guard that sometimes guards is worse than one that says it cannot.
//!
//! It denies writes without exception, including caches and temporary files. A
//! test run that wants to write `__pycache__` will find it cannot. That is the
//! cost of the guarantee and it is not softened here: an exception is a path
//! where an unrecorded change can happen, which is the thing being removed.

use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "linux")]
pub mod landlock;

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
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
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
    if !cfg!(any(target_os = "macos", target_os = "linux")) {
        return Err(unsupported());
    }
    // A Linux without Landlock is refused here rather than at the point of no
    // return, so the command never starts believing it was guarded.
    #[cfg(target_os = "linux")]
    if landlock::abi_version().is_none() {
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

    // macOS needs a profile file for `sandbox-exec`; Linux builds its ruleset in
    // memory and needs nothing on disk.
    #[cfg(target_os = "macos")]
    let profile_path = {
        let path = paths.root().join("guard.sb");
        std::fs::write(&path, profile(&root)).map_err(|error| {
            Error::Faulted(Fault::io("could not write the sandbox profile", error))
        })?;
        path
    };
    #[cfg(not(target_os = "macos"))]
    let profile_path = std::path::PathBuf::new();

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

    let status = sandboxed(program, arguments, &root, &profile_path).and_then(|mut command| {
        command
            .env(SOCKET_ENV, &socket_path)
            .current_dir(&root)
            .status()
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => unsupported(),
                _ => Error::Faulted(Fault::io("could not start the guarded command", error)),
            })
    });

    // Stopped before the result is examined, so a failure to start the command
    // does not leave an engine listening on a socket nobody will connect to.
    let _ = stop.send(());
    runtime.block_on(async {
        let _ = engine.await;
    });
    let _ = std::fs::remove_file(&socket_path);

    // A signalled child has no exit code. Reporting 0 would say it succeeded.
    Ok(status?.code().unwrap_or(1))
}

/// The command, wrapped in whatever this kernel uses to take the workspace away.
///
/// macOS runs it under `sandbox-exec` with a profile file. Linux applies a
/// Landlock ruleset between fork and exec, which needs no helper binary — and
/// must not be applied any earlier, because this process is the engine and is
/// the one thing that still has to be able to write there.
#[cfg(target_os = "macos")]
fn sandboxed(program: &str, arguments: &[String], _root: &Path, profile: &Path) -> Result<Command> {
    let mut command = Command::new("sandbox-exec");
    command.arg("-f").arg(profile).arg(program).args(arguments);
    Ok(command)
}

#[cfg(target_os = "linux")]
fn sandboxed(program: &str, arguments: &[String], root: &Path, _profile: &Path) -> Result<Command> {
    use std::os::fd::AsRawFd as _;
    use std::os::unix::process::CommandExt as _;

    let mut command = Command::new(program);
    command.args(arguments);

    // Built here, in the parent, because building it allocates and reads
    // directories — neither of which is allowed after a fork. What runs in the
    // child is two syscalls on a descriptor that is already open.
    //
    // A failure here is reported as itself. Replacing the command with one that
    // cannot run would keep it from starting unguarded, but the resulting
    // "not found" was then read as "this platform has no sandbox" — which is
    // false on a kernel that has Landlock and sends a person to fix the wrong
    // thing.
    let ruleset = landlock::build(root).map_err(|error| {
        Error::Faulted(Fault::io("could not build the Landlock ruleset", error))
    })?;

    // SAFETY: the closure runs between fork and exec and does nothing but call
    // `prctl` and `landlock_restrict_self`, both of which are async-signal-safe.
    // `ruleset` is moved in and stays open for the lifetime of the command.
    unsafe {
        command.pre_exec(move || restrict_current_thread(ruleset.as_raw_fd()));
    }
    Ok(command)
}

#[cfg(target_os = "linux")]
fn restrict_current_thread(ruleset: std::os::fd::RawFd) -> std::io::Result<()> {
    // SAFETY: called from `pre_exec`, which is between fork and exec, with a
    // descriptor this process opened and still holds.
    unsafe { landlock::restrict_current_thread(ruleset) }
}

/// Where the guard would put its socket, for a workspace.
pub fn socket_path(workspace: &Path) -> Result<PathBuf> {
    socket::path_for(registry::load(workspace)?.id())
}
