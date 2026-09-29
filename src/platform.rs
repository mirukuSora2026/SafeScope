//! The file operations the executor is built from.
//!
//! Every write reaches its final name through a rename, so a reader never sees a
//! half-written file. Every rename is followed by an fsync of the directory,
//! because without it the rename itself can be lost in a crash while the caller
//! believes the write succeeded.
//!
//! # Refusing rather than emulating
//!
//! Two operations are refused when the platform cannot perform them properly,
//! instead of being approximated:
//!
//! - **Renaming without overwriting.** If the filesystem has no atomic
//!   no-overwrite rename, checking first and renaming after would reopen exactly
//!   the race the flag exists to close. A caller that is told "no" can ask a
//!   person; a caller handed a racy emulation cannot tell the difference.
//! - **Moving across filesystems.** `rename` cannot do it, and copy-then-delete
//!   is not one operation: a crash in the middle leaves the file in two places
//!   or none, and the journal would have recorded a move.

// `renameatx_np` takes raw descriptors and C strings; the Linux path takes
// neither, so these are scoped to the platform that uses them rather than
// tolerated as unused everywhere else.
#[cfg(target_os = "macos")]
use std::ffi::CString;
use std::io::Write as _;
#[cfg(unix)]
use std::os::fd::AsFd;
#[cfg(target_os = "macos")]
use std::os::fd::AsRawFd;

use cap_std::fs::Dir;

#[cfg(windows)]
pub mod windows;

use crate::dataformatting::Msg;
use crate::error::{Denial, Error, ErrorCode, Fault, Result};
use crate::paths::TMP_PREFIX;

/// A file written but not yet under its final name.
///
/// Dropping one removes it. Anything that fails between writing and renaming
/// therefore leaves no debris, and the only temporaries recovery has to clean up
/// are those left by a crash.
#[derive(Debug)]
pub struct Staged<'a> {
    directory: &'a Dir,
    name: String,
    promoted: bool,
}

impl<'a> Staged<'a> {
    /// The temporary's name, which recovery matches against [`TMP_PREFIX`].
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Renames over whatever is there. Used by `replace`, where overwriting is
    /// the point and the previous contents are already in the snapshot store.
    pub fn promote_over(mut self, name: &str) -> Result<()> {
        rename_within(self.directory, &self.name, name, Overwrite::Allowed)?;
        self.promoted = true;
        fsync(self.directory, "fsync")
    }

    /// Renames only if nothing is there. Used by `create`, where an existing
    /// file would be destroyed without ever having been snapshotted.
    pub fn promote_new(mut self, name: &str) -> Result<()> {
        rename_within(self.directory, &self.name, name, Overwrite::Refused)?;
        self.promoted = true;
        fsync(self.directory, "fsync")
    }
}

impl Drop for Staged<'_> {
    fn drop(&mut self) {
        if !self.promoted {
            let _ = self.directory.remove_file(&self.name);
        }
    }
}

/// Writes `contents` to a temporary in `directory`, durably.
///
/// The temporary sits beside its eventual target so the rename stays within one
/// filesystem, which is what makes it atomic.
pub fn stage<'a>(directory: &'a Dir, contents: &[u8]) -> Result<Staged<'a>> {
    let name = format!("{TMP_PREFIX}{}", uuid::Uuid::new_v4().simple());
    let staged = Staged {
        directory,
        name,
        promoted: false,
    };

    let mut file = directory
        .create(&staged.name)
        .map_err(|error| failed("create", &error))?;
    file.write_all(contents)
        .map_err(|error| failed("write", &error))?;
    // The contents must be on disk before the rename, or a crash can leave the
    // target name pointing at an empty file.
    file.sync_all().map_err(|error| failed("fsync", &error))?;

    Ok(staged)
}

/// Moves a file between directories without overwriting the destination.
pub fn move_file(
    from_directory: &Dir,
    from_name: &str,
    to_directory: &Dir,
    to_name: &str,
) -> Result<()> {
    rename_between(from_directory, from_name, to_directory, to_name)?;
    fsync(from_directory, "fsync")?;
    fsync(to_directory, "fsync")
}

/// Removes a file and makes the removal durable.
pub fn remove(directory: &Dir, name: &str) -> Result<()> {
    directory
        .remove_file(name)
        .map_err(|error| failed("unlink", &error))?;
    fsync(directory, "fsync")
}

/// Flushes a directory's own metadata, so a rename or unlink survives a crash.
///
/// The descriptor is reopened rather than flushed directly. On Linux `cap-std`
/// opens a directory with `O_PATH`, which names a file without granting access
/// to it, and `fsync` on one fails with `EBADF`. macOS has no `O_PATH`, so the
/// direct call worked there and every durability guarantee in this file was
/// quietly unenforced on Linux.
///
/// Reopening through the same descriptor keeps the capability — `openat` on
/// `"."` cannot escape the directory it is relative to — while producing one the
/// kernel will actually flush.
#[cfg(unix)]
pub fn fsync(directory: &Dir, operation: &str) -> Result<()> {
    let flushable = rustix::fs::openat(
        directory.as_fd(),
        ".",
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(|error| failed(operation, &std::io::Error::from(error)))?;

    rustix::fs::fsync(&flushable).map_err(|error| failed(operation, &std::io::Error::from(error)))
}

/// Windows has no directory flush; see [`windows::flush_directory`] for what
/// that costs and why it is not emulated.
#[cfg(windows)]
pub fn fsync(directory: &Dir, operation: &str) -> Result<()> {
    windows::flush_directory(directory).map_err(|error| failed(operation, &error))
}

/// Whether the destination may be replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overwrite {
    Allowed,
    Refused,
}

fn rename_within(
    directory: &Dir,
    from_name: &str,
    to_name: &str,
    overwrite: Overwrite,
) -> Result<()> {
    rename(directory, from_name, directory, to_name, overwrite)
}

fn rename_between(
    from_directory: &Dir,
    from_name: &str,
    to_directory: &Dir,
    to_name: &str,
) -> Result<()> {
    rename(
        from_directory,
        from_name,
        to_directory,
        to_name,
        Overwrite::Refused,
    )
}

fn rename(
    from_directory: &Dir,
    from_name: &str,
    to_directory: &Dir,
    to_name: &str,
    overwrite: Overwrite,
) -> Result<()> {
    if overwrite == Overwrite::Allowed {
        return from_directory
            .rename(from_name, to_directory, to_name)
            .map_err(|error| map_rename_error(&error, from_name, to_name));
    }
    rename_no_replace(from_directory, from_name, to_directory, to_name)
        .map_err(|error| map_rename_error(&error, from_name, to_name))
}

/// Renames only if the destination does not exist, atomically.
///
/// Both implementations use the kernel's own flag. Neither falls back to a
/// check followed by a plain rename: that is the race this exists to close.
#[cfg(target_os = "linux")]
fn rename_no_replace(
    from_directory: &Dir,
    from_name: &str,
    to_directory: &Dir,
    to_name: &str,
) -> std::io::Result<()> {
    rustix::fs::renameat_with(
        from_directory.as_fd(),
        from_name,
        to_directory.as_fd(),
        to_name,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(std::io::Error::from)
}

/// The macOS spelling of the same thing: `renameatx_np` with `RENAME_EXCL`.
#[cfg(target_os = "macos")]
fn rename_no_replace(
    from_directory: &Dir,
    from_name: &str,
    to_directory: &Dir,
    to_name: &str,
) -> std::io::Result<()> {
    let from = CString::new(from_name)?;
    let to = CString::new(to_name)?;

    // SAFETY: both descriptors are borrowed from live `Dir` values for the
    // duration of the call, and both strings are NUL-terminated and outlive it.
    let outcome = unsafe {
        libc::renameatx_np(
            from_directory.as_fd().as_raw_fd(),
            from.as_ptr(),
            to_directory.as_fd().as_raw_fd(),
            to.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    if outcome == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Turns a rename failure into something a caller can act on.
/// The Windows spelling: `SetFileInformationByHandle` with `FileRenameInfo`,
/// which is relative to a directory handle and refuses an existing destination
/// without a separate check.
#[cfg(windows)]
fn rename_no_replace(
    from_directory: &Dir,
    from_name: &str,
    to_directory: &Dir,
    to_name: &str,
) -> std::io::Result<()> {
    windows::rename_no_replace(from_directory, from_name, to_directory, to_name)
}

/// Whether a rename here is durable once it returns.
///
/// False where the platform cannot flush a directory. [`crate::cli::report`]
/// asks this rather than testing the target itself, so the answer and the
/// sentence a person reads cannot drift apart — which they already had once,
/// with a module claiming the status output said something it never said.
pub const fn rename_is_durable() -> bool {
    !cfg!(windows)
}

/// Turns what a rename failed with into what the caller should be told.
///
/// Public because it is a pure translation and the only place an error shape
/// becomes a decision. Testing it through a real rename can only reach the
/// shapes the platform under the test produces, and the one that was wrong was
/// on the platform that could not be run.
///
/// `kind` is checked before the errno. An error that arrived already classified
/// carries no errno at all — `io::Error::new` wraps rather than replaces — so
/// matching the number first sent Windows's "destination is taken" down the
/// fault path, where a refusal was reported as an engine failure and a caller
/// could not tell that retrying was pointless.
pub fn map_rename_error(error: &std::io::Error, from_name: &str, to_name: &str) -> Error {
    match error.kind() {
        std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::DirectoryNotEmpty => {
            return Error::Denied(Denial::new(
                ErrorCode::DestinationExists,
                Msg::PathDestinationExists {
                    path: to_name.to_owned(),
                },
            ));
        }
        std::io::ErrorKind::CrossesDevices => {
            return Error::Denied(Denial::new(
                ErrorCode::UnsupportedOperation,
                Msg::PlatformCrossFilesystem {
                    from: from_name.to_owned(),
                    to: to_name.to_owned(),
                },
            ));
        }
        // The filesystem cannot refuse to overwrite. Emulating that would mean
        // checking first, which is the race the refusal exists to close.
        std::io::ErrorKind::Unsupported => {
            return Error::Denied(Denial::new(
                ErrorCode::UnsupportedOperation,
                Msg::PlatformAtomicRenameUnsupported {
                    reason: error.to_string(),
                },
            ));
        }
        _ => {}
    }

    // An errno means something only where the platform reports errno. Windows
    // puts a Win32 code in the same field and the numbers overlap — EEXIST is
    // 17, which there is ERROR_NOT_SAME_DEVICE, and EXDEV is 18, which is "no
    // more files" — so matching them there would classify one failure as
    // another. Its codes become kinds, through std and `windows::translate`,
    // and were matched above.
    if !cfg!(unix) {
        return failed("rename", error);
    }

    match error.raw_os_error() {
        Some(libc::EEXIST) | Some(libc::ENOTEMPTY) => Error::Denied(Denial::new(
            ErrorCode::DestinationExists,
            Msg::PathDestinationExists {
                path: to_name.to_owned(),
            },
        )),
        Some(libc::EXDEV) => Error::Denied(Denial::new(
            ErrorCode::UnsupportedOperation,
            Msg::PlatformCrossFilesystem {
                from: from_name.to_owned(),
                to: to_name.to_owned(),
            },
        )),
        // The filesystem does not implement the no-overwrite flag. Emulating it
        // would mean checking first, which is the race the flag exists to close.
        Some(libc::ENOTSUP) | Some(libc::EINVAL) | Some(libc::ENOSYS) => {
            Error::Denied(Denial::new(
                ErrorCode::UnsupportedOperation,
                Msg::PlatformAtomicRenameUnsupported {
                    reason: error.to_string(),
                },
            ))
        }
        _ => failed("rename", error),
    }
}

fn failed(operation: &str, error: &std::io::Error) -> Error {
    Error::Faulted(Fault::new(
        ErrorCode::IoFailed,
        Msg::PlatformOperationFailed {
            operation: operation.to_owned(),
            reason: error.to_string(),
        },
    ))
}
