//! Serving and reaching the engine across a Unix socket.
//!
//! The reason this exists is [`crate::guard`]. A sandbox that denies the agent
//! every write to the workspace also denies them to anything the agent starts,
//! and the MCP server is started by the agent's host — so a server on stdio
//! would be inside the sandbox and unable to carry out the changes it approved.
//!
//! So the engine runs in the guard process, outside, and listens here. What the
//! host starts is a relay: it copies bytes between its own stdio and this
//! socket, writes nothing, and is therefore unharmed by the sandbox.
//!
//! The socket lives outside the workspace, so the rule denying writes to the
//! workspace does not deny reaching it. Not beside the state either: a Unix
//! socket path has a hard length limit — about a hundred bytes — and the state
//! directory is nested deep enough under a data root that a perfectly ordinary
//! workspace overran it. It goes in the per-user temporary directory instead,
//! whose length is bounded and whose permissions are already the user's own.
//!
//! One connection at a time, in sequence. The engine holds the workspace lock
//! for its lifetime, so a second concurrent client could not do anything anyway,
//! and serving them in turn makes that explicit rather than mysterious.

use std::path::{Path, PathBuf};

use tokio::net::{UnixListener, UnixStream};

use crate::dataformatting::Msg;
use crate::error::{Error, ErrorCode, Fault, Result};
use crate::ids::WorkspaceId;

use super::SafeScope;

/// The environment variable naming the socket to reach.
///
/// Set by the guard for everything it starts. Its presence is what makes
/// `safescope mcp` relay rather than serve, which keeps one manifest working
/// both ways — a plugin that had to be edited to be guarded would be a plugin
/// people run unguarded.
pub const SOCKET_ENV: &str = "SAFESCOPE_MCP_SOCKET";

fn failed(reason: impl std::fmt::Display) -> Error {
    Error::Faulted(Fault::new(
        ErrorCode::IoFailed,
        Msg::McpTransportFailed {
            reason: reason.to_string(),
        },
    ))
}

/// The longest a Unix socket path may be.
///
/// `sockaddr_un.sun_path` is 104 bytes on macOS and 108 on Linux; the shorter
/// is used so a path that works here works there. The failure without this
/// check is `path must be shorter than SUN_LEN`, from inside a transport, which
/// says nothing about which path or why.
const MAX_SOCKET_PATH: usize = 100;

/// Where a workspace's guard listens.
///
/// Named from the workspace identity rather than its path: two checkouts of the
/// same project are different workspaces and must not share a socket, and the
/// identity already says which is which.
pub fn path_for(id: WorkspaceId) -> Result<PathBuf> {
    let name = id.to_string();
    let short = name.rsplit('_').next().unwrap_or(&name);
    let path =
        std::env::temp_dir().join(format!("safescope-{}.sock", &short[..16.min(short.len())]));

    if path.as_os_str().len() > MAX_SOCKET_PATH {
        return Err(failed(format!(
            "the socket path {} is too long for this platform",
            path.display()
        )));
    }
    Ok(path)
}

/// Binds the socket, replacing one left by a process that is gone — and only one.
///
/// A socket file outlives the process that bound it, so a crash leaves one that
/// nothing is listening on. Unlinking unconditionally would also take the socket
/// of a guard that is very much alive: the second guard on a workspace would
/// silently cut the first one's engine off from its own agent, which would then
/// find its tool calls answered by somebody else's engine.
///
/// So a live socket is told from an abandoned one by connecting to it. A refused
/// connection means nothing is listening and the file is debris; a connection
/// that is accepted means another guard has this workspace, which is reported
/// rather than taken.
///
/// This is the mutual exclusion, not the workspace lock. The guard cannot hold
/// that — the engine it runs takes it for each session, and a process cannot
/// take a lock it is already holding.
pub fn bind(path: &Path) -> Result<UnixListener> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(path).is_ok() {
            return Err(Error::Faulted(
                Fault::new(
                    ErrorCode::WorkspaceBusy,
                    Msg::WorkspaceBusyElsewhere {
                        path: path.display().to_string(),
                    },
                )
                .with_hint(Msg::HintAnotherSessionIsWriting),
            ));
        }
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(failed(error)),
        }
    }
    let listener = UnixListener::bind(path).map_err(failed)?;

    // Only this user may connect. The socket is the one channel that can change
    // the workspace while it is sealed, so leaving it open to anyone who can
    // reach the path would put the guarantee back in somebody else's hands.
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .map_err(failed)?;
    Ok(listener)
}

/// Answers clients on `listener`, one after another, until `shutdown` fires.
///
/// The engine is rebuilt per connection so a client that disconnects mid-change
/// cannot leave the next one holding its half-finished view. The workspace lock
/// and the journal outlive it, which is what makes that safe.
pub async fn serve(
    root: PathBuf,
    listener: UnixListener,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<()> {
    loop {
        let stream = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                // A failed accept says nothing about the next one, and the guard
                // is holding a child process open that still needs answering.
                Err(_) => continue,
            },
            _ = &mut shutdown => return Ok(()),
        };
        // Not `?`: one connection failing must not stop the guard answering the
        // next. The guard has already told the person the workspace is sealed,
        // so an engine that quietly stopped listening would leave every later
        // tool call hitting a dead socket with no way through.
        let Ok(server) = SafeScope::open(&root) else {
            continue;
        };
        let running = match rmcp::ServiceExt::serve(server, stream).await {
            Ok(running) => running,
            // A client that hangs up during the handshake is not this server's
            // problem to report; the next one may be fine.
            Err(_) => continue,
        };
        let _ = running.waiting().await;
    }
}

/// Copies bytes between this process's stdio and the socket.
///
/// Deliberately not an MCP implementation: it does not parse a single message.
/// Anything it understood would be a second place the protocol lives, and a
/// second place for it to be wrong.
pub async fn relay(path: &Path) -> Result<()> {
    let stream = UnixStream::connect(path).await.map_err(failed)?;
    let (mut from_engine, mut to_engine) = stream.into_split();
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    let up = async { tokio::io::copy(&mut stdin, &mut to_engine).await };
    let down = async { tokio::io::copy(&mut from_engine, &mut stdout).await };

    // Whichever direction closes first ends the relay: a half-open pipe between
    // a host and an engine is a session that appears alive and answers nothing.
    tokio::select! {
        result = up => result.map_err(failed)?,
        result = down => result.map_err(failed)?,
    };
    Ok(())
}

/// The socket to relay to, if this process was started under a guard.
pub fn socket_from_env() -> Option<PathBuf> {
    std::env::var_os(SOCKET_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}
