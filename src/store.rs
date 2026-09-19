//! Where engine state lives, and how it is written.
//!
//! State sits outside the workspace. Keeping snapshots and the journal inside the
//! tree they protect would put recovery data at risk from the very operations it
//! exists to undo, and would leak engine state into the user's version control.
//!
//! Every write here goes through [`write_atomically`]: a temporary file, an
//! fsync, a rename, then an fsync of the directory. A half-written approved
//! policy or journal record is worse than none, because the engine would read it
//! and believe it.

pub mod content;
pub mod lock;
pub mod policy_store;

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::dataformatting::Msg;
use crate::error::{Denial, Error, ErrorCode, Fault, Result};
use crate::ids::WorkspaceId;

/// Overrides where engine state is kept.
///
/// Tests rely on this, and so does anyone whose home directory is not where they
/// want this data.
pub const DATA_DIR_ENV: &str = "SAFESCOPE_DATA_DIR";

/// Directory name under the platform's data location.
const APP_DIR: &str = "safescope";

/// The root of all engine state.
pub fn data_root() -> Result<PathBuf> {
    if let Ok(explicit) = std::env::var(DATA_DIR_ENV)
        && !explicit.is_empty()
    {
        return Ok(PathBuf::from(explicit));
    }

    let home = std::env::var_os("HOME").map(PathBuf::from);
    let platform_root = if cfg!(target_os = "macos") {
        home.map(|home| home.join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| home.map(|home| home.join(".local/share")))
    };

    platform_root.map(|root| root.join(APP_DIR)).ok_or_else(|| {
        Error::Faulted(Fault::new(
            ErrorCode::IoFailed,
            Msg::StoreDataDirectoryUnavailable,
        ))
    })
}

/// The state directory for one workspace.
#[derive(Debug, Clone)]
pub struct StatePaths {
    root: PathBuf,
}

impl StatePaths {
    /// Locates state for a workspace, refusing to place it inside that workspace.
    pub fn for_workspace(id: WorkspaceId, workspace_root: &Path) -> Result<Self> {
        let root = data_root()?
            .join("workspaces")
            .join(id.as_uuid().simple().to_string());

        // Snapshots inside the tree they protect would be at risk from the very
        // operations they exist to undo.
        if let (Ok(state), Ok(workspace)) = (absolutise(&root), workspace_root.canonicalize())
            && state.starts_with(&workspace)
        {
            return Err(Error::Denied(Denial::new(
                ErrorCode::WorkspaceStateUnusable,
                Msg::StoreStateInsideWorkspace {
                    state: state.display().to_string(),
                    workspace: workspace.display().to_string(),
                },
            )));
        }

        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Approved policy snapshots, one file per version.
    pub fn approved_policy(&self) -> PathBuf {
        self.root.join("approved-policy")
    }

    /// Content-addressed recovery data.
    pub fn snapshots(&self) -> PathBuf {
        self.root.join("snapshots")
    }

    /// Payloads held between planning and applying.
    pub fn staging(&self) -> PathBuf {
        self.root.join("staging")
    }

    /// The journal database.
    pub fn journal(&self) -> PathBuf {
        self.root.join("state.sqlite")
    }

    /// The advisory lock guarding single-writer access.
    pub fn lock(&self) -> PathBuf {
        self.root.join("lock")
    }

    /// Creates the directories that must exist before anything is written.
    pub fn create(&self) -> Result<()> {
        for directory in [
            self.root.clone(),
            self.approved_policy(),
            self.snapshots(),
            self.staging(),
        ] {
            fs::create_dir_all(&directory).map_err(|error| write_failed(&directory, &error))?;
        }
        Ok(())
    }
}

/// Resolves a path that may not exist yet, without requiring it to.
fn absolutise(path: &Path) -> std::io::Result<PathBuf> {
    match path.canonicalize() {
        Ok(resolved) => Ok(resolved),
        Err(_) => {
            // The state directory usually does not exist yet, so fall back to
            // resolving the nearest existing ancestor and re-appending the rest.
            let mut existing = path;
            let mut trailing = PathBuf::new();
            loop {
                match existing.parent() {
                    Some(parent) => {
                        let name = existing.file_name().map(PathBuf::from).unwrap_or_default();
                        trailing = if trailing.as_os_str().is_empty() {
                            name
                        } else {
                            name.join(&trailing)
                        };
                        if let Ok(resolved) = parent.canonicalize() {
                            return Ok(resolved.join(trailing));
                        }
                        existing = parent;
                    }
                    None => return Ok(path.to_path_buf()),
                }
            }
        }
    }
}

/// Writes a file so a reader never sees it half-written.
///
/// A partly written approved policy or journal record is worse than none at all,
/// because the engine would read it and act on it.
pub fn write_atomically(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        Error::Faulted(Fault::new(
            ErrorCode::IoFailed,
            Msg::StoreWriteFailed {
                path: path.display().to_string(),
                reason: "the path has no parent directory".to_owned(),
            },
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| write_failed(parent, &error))?;

    // The temporary sits beside the target so the rename stays within one
    // filesystem and is therefore atomic.
    let temporary = parent.join(format!(".sfs-tmp-{}", uuid::Uuid::new_v4().simple()));

    let outcome = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        // Without this the rename itself can be lost in a crash, leaving the old
        // contents in place while the caller believes the write succeeded.
        fs::File::open(parent)?.sync_all()
    })();

    if let Err(error) = outcome {
        let _ = fs::remove_file(&temporary);
        return Err(write_failed(path, &error));
    }
    Ok(())
}

pub(crate) fn write_failed(path: &Path, error: &std::io::Error) -> Error {
    Error::Faulted(Fault::new(
        ErrorCode::IoFailed,
        Msg::StoreWriteFailed {
            path: path.display().to_string(),
            reason: error.to_string(),
        },
    ))
}

pub(crate) fn read_failed(path: &Path, error: &std::io::Error) -> Error {
    Error::Faulted(Fault::new(
        ErrorCode::IoFailed,
        Msg::StoreReadFailed {
            path: path.display().to_string(),
            reason: error.to_string(),
        },
    ))
}

pub(crate) fn corrupted(path: &Path, reason: impl std::fmt::Display) -> Error {
    Error::Faulted(Fault::new(
        ErrorCode::JournalFailed,
        Msg::StoreCorrupted {
            path: path.display().to_string(),
            reason: reason.to_string(),
        },
    ))
}
