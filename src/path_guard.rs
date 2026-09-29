//! Resolving a validated path to a real place on disk.
//!
//! This module upholds invariant I4: **path decisions and execution share one
//! directory handle**. Checking a path by name and then acting on it by name
//! leaves a window in which the name can come to mean something else. Resolution
//! therefore hands back the parent directory handle it descended through, and the
//! executor acts through that handle rather than re-walking the path.
//!
//! Symbolic links are refused rather than followed. A link is a perfectly good
//! filesystem feature, but following one means the operation lands somewhere other
//! than the path that was checked against the policy, and no amount of care after
//! that point recovers the guarantee.

use std::path::{Path, PathBuf};

use cap_fs_ext::DirExt as _;
use cap_std::ambient_authority;
use cap_std::fs::Dir;

use crate::dataformatting::Msg;
use crate::domain::FileState;
use crate::error::{Denial, Error, ErrorCode, Fault, Result};
use crate::hash::ContentHash;
use crate::paths::RelPath;

/// An open workspace root.
///
/// The root is opened once and held. Every resolution descends from this handle,
/// so a path can never be interpreted against a different directory than the one
/// the workspace was registered for.
#[derive(Debug)]
pub struct Workspace {
    root: Dir,
    display: PathBuf,
}

impl Workspace {
    /// Opens a workspace root.
    pub fn open(root: &Path) -> Result<Self> {
        let handle = Dir::open_ambient_dir(root, ambient_authority()).map_err(|error| {
            Error::Faulted(
                Fault::new(
                    ErrorCode::IoFailed,
                    Msg::WorkspaceOpenFailed {
                        root: root.display().to_string(),
                        reason: error.to_string(),
                    },
                )
                .with_source(error),
            )
        })?;
        Ok(Self {
            root: handle,
            display: root.to_path_buf(),
        })
    }

    /// The root path, for display only. Never for resolution.
    pub fn display(&self) -> &Path {
        &self.display
    }

    pub const fn root(&self) -> &Dir {
        &self.root
    }

    /// Resolves a path, returning its parent handle and current state.
    ///
    /// The path must already have passed [`RelPath::parse`]; this adds what only
    /// the filesystem can answer — whether each component is what it claims to
    /// be, and what is actually there now.
    pub fn resolve(&self, path: &RelPath) -> Result<Resolved> {
        let mut parent = self.root.try_clone().map_err(|error| {
            Error::Faulted(Fault::io("could not duplicate the workspace handle", error))
        })?;

        let components: Vec<&str> = path.components().collect();
        let (file_name, directories) = components
            .split_last()
            .expect("RelPath has at least one component");

        for component in directories {
            match parent.symlink_metadata(component) {
                Ok(metadata) if metadata.is_symlink() => {
                    return Err(symlink_refused(path));
                }
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(Error::Denied(Denial::new(
                        ErrorCode::UnsupportedOperation,
                        Msg::PathComponentNotADirectory {
                            path: path.as_str().to_owned(),
                            component: (*component).to_owned(),
                        },
                    )));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(parent_missing(path));
                }
                Err(error) => {
                    return Err(Error::Faulted(Fault::io(
                        "could not inspect a path component",
                        error,
                    )));
                }
            }

            // `open_dir_nofollow`, not `open_dir`. The plain one follows a
            // symlinked component — cap-std resolves links itself before it
            // opens anything — so the check above and the open below were two
            // questions about a name that can change in between, and a link put
            // there after the check would land the whole operation in a
            // directory the policy never saw.
            parent = parent.open_dir_nofollow(component).map_err(|error| {
                if is_a_symlink(&error) {
                    return symlink_refused(path);
                }
                Error::Faulted(Fault::io("could not open a path component", error))
            })?;
        }

        let state = observe(&parent, file_name, path)?;
        Ok(Resolved {
            path: path.clone(),
            parent,
            file_name: (*file_name).to_owned(),
            state,
        })
    }
}

/// A path resolved against the filesystem.
#[derive(Debug)]
pub struct Resolved {
    path: RelPath,
    parent: Dir,
    file_name: String,
    state: FileState,
}

impl Resolved {
    pub const fn path(&self) -> &RelPath {
        &self.path
    }

    /// The directory the target lives in. The executor acts through this handle
    /// rather than re-walking the path, which is what makes I4 hold.
    pub const fn parent(&self) -> &Dir {
        &self.parent
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// What was actually there at the moment of resolution.
    pub const fn state(&self) -> &FileState {
        &self.state
    }

    pub const fn exists(&self) -> bool {
        self.state.exists()
    }
}

/// Reads the current state of the final component.
///
/// Opened first, and described from the open handle. Asking the path what it is
/// and then opening it by name is two questions about a name that can change in
/// between: a target that became a symlink after the check would be followed by
/// the open — cap-std follows them by default — and the contents hashed, stored
/// as the snapshot and later restored by undo would belong to a different file.
/// One call decides, and the thing described is the thing that was hashed.
fn observe(parent: &Dir, file_name: &str, path: &RelPath) -> Result<FileState> {
    let file = match open_without_following(parent, file_name) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FileState::Absent);
        }
        Err(error) if is_a_symlink(&error) => return Err(symlink_refused(path)),
        Err(error) => {
            return Err(Error::Faulted(Fault::io(
                "could not inspect the target",
                error,
            )));
        }
    };

    let metadata = file
        .metadata()
        .map_err(|error| Error::Faulted(Fault::io("could not inspect the target", error)))?;

    // Not reached where the open refuses a link, which cap-std does on both
    // Unix and Windows. Kept so that a platform whose no-follow open hands back
    // the link itself still refuses it rather than hashing a reparse point.
    if metadata.is_symlink() {
        return Err(symlink_refused(path));
    }
    if !metadata.is_file() {
        return Err(Error::Denied(Denial::new(
            ErrorCode::UnsupportedOperation,
            Msg::PathNotARegularFile {
                path: path.as_str().to_owned(),
            },
        )));
    }

    let (hash, len) = ContentHash::of_reader(file)
        .map_err(|error| Error::Faulted(Fault::io("could not hash the target", error)))?;
    Ok(FileState::present(hash, len))
}

/// Opens a file without following a symlink at the final component.
///
/// The whole of what keeps a swapped target from being read. `cap-std` opens
/// with `FollowSymlinks::Yes` by default, so this is asked for explicitly.
pub fn open_without_following(parent: &Dir, file_name: &str) -> std::io::Result<cap_std::fs::File> {
    use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt as _, OpenOptionsMaybeDirExt as _};

    // Through cap-std's own setting, not a raw `O_NOFOLLOW`. cap-std resolves
    // symlinks itself before it opens anything, so a flag passed to the syscall
    // arrives after the link has already been followed — which looked like it
    // worked and did nothing.
    //
    // `maybe_dir` so that a directory opens rather than failing. Unix opens one
    // for reading anyway; Windows refuses with "access denied" unless asked for
    // backup semantics, which made a directory target a fault — the engine
    // failing — rather than the refusal it is. Opened, it is described by its
    // handle like anything else and refused as not a regular file.
    let mut options = cap_std::fs::OpenOptions::new();
    options
        .read(true)
        .follow(FollowSymlinks::No)
        .maybe_dir(true);
    parent.open_with(file_name, &options)
}

/// Whether an open failed because the name was a symlink.
///
/// `O_NOFOLLOW` reports it as a loop. It is the same answer as the check this
/// replaced, arrived at without a window between asking and acting.
#[cfg(unix)]
fn is_a_symlink(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(libc::ELOOP)
}

/// On Windows cap-std opens the reparse point, sees a link, and refuses with
/// ERROR_STOPPED_ON_SYMLINK — its stand-in for `O_NOFOLLOW`'s loop. Reporting
/// that as anything else made a symlink the engine failing rather than a
/// refusal.
#[cfg(windows)]
fn is_a_symlink(error: &std::io::Error) -> bool {
    /// ERROR_STOPPED_ON_SYMLINK.
    const STOPPED_ON_SYMLINK: i32 = 681;
    error.raw_os_error() == Some(STOPPED_ON_SYMLINK)
}

#[cfg(not(any(unix, windows)))]
fn is_a_symlink(_error: &std::io::Error) -> bool {
    false
}

fn symlink_refused(path: &RelPath) -> Error {
    Error::Denied(Denial::new(
        ErrorCode::UnsupportedOperation,
        Msg::PathSymlinkRefused {
            path: path.as_str().to_owned(),
        },
    ))
}

fn parent_missing(path: &RelPath) -> Error {
    let parent = path
        .parent()
        .map_or_else(|| ".".to_owned(), |parent| parent.as_str().to_owned());
    Error::Denied(
        Denial::new(
            ErrorCode::ParentMissing,
            Msg::PathParentMissing {
                path: path.as_str().to_owned(),
                parent,
            },
        )
        .with_hint(Msg::HintCreateTheDirectoryFirst),
    )
}
