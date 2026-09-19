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

            parent = parent.open_dir(component).map_err(|error| {
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
fn observe(parent: &Dir, file_name: &str, path: &RelPath) -> Result<FileState> {
    let metadata = match parent.symlink_metadata(file_name) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FileState::Absent);
        }
        Err(error) => {
            return Err(Error::Faulted(Fault::io(
                "could not inspect the target",
                error,
            )));
        }
    };

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

    let file = parent
        .open(file_name)
        .map_err(|error| Error::Faulted(Fault::io("could not read the target", error)))?;
    let (hash, len) = ContentHash::of_reader(file)
        .map_err(|error| Error::Faulted(Fault::io("could not hash the target", error)))?;
    Ok(FileState::present(hash, len))
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
