//! One writer at a time, per workspace.
//!
//! The budget is checked and then reserved as two steps. That is only sound if
//! nothing can spend budget in between, and the same goes for reading a file's
//! state and acting on it. This lock is what makes "single writer" true rather
//! than assumed.
//!
//! It is advisory and scoped to SafeScope. It coordinates SafeScope processes
//! with each other; it does not stop an editor, a build, or a shell command from
//! writing to the same files. Nothing in this engine claims otherwise.
//!
//! Acquiring never waits. A caller that finds the workspace busy should say so
//! and let a person decide, because the thing it would be waiting for is another
//! session that may be sitting at a prompt.

use std::fs::File;
use std::path::{Path, PathBuf};

use rustix::fs::{FlockOperation, flock};

use crate::dataformatting::Msg;
use crate::error::{Error, ErrorCode, Fault, Result};

use super::{StatePaths, write_failed};

/// An exclusive hold on one workspace, released when dropped.
#[derive(Debug)]
pub struct WorkspaceLock {
    /// Held open for as long as the lock is: the lock belongs to this open file
    /// description, so closing the file would release it.
    _file: File,
    path: PathBuf,
}

impl WorkspaceLock {
    /// Takes the lock, or reports that somebody else has it.
    pub fn acquire(paths: &StatePaths) -> Result<Self> {
        Self::acquire_at(&paths.lock())
    }

    pub fn acquire_at(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| write_failed(parent, &error))?;
        }
        let file = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
            .map_err(|error| write_failed(path, &error))?;

        // Never blocking: what it would wait for is another session that may be
        // sitting at a prompt, and a person should be told rather than hung.
        // Another session holding the lock is a condition of the environment,
        // not a rule saying no — and unlike a denial, retrying it is meaningful.
        //
        // Only EWOULDBLOCK means somebody else has it. Reporting every errno as
        // a busy workspace once made a signal arriving mid-call — EINTR, which
        // says nothing about the lock — look like a second session, and the
        // advice that came with it was to close a session that did not exist.
        loop {
            match flock(&file, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => break,
                Err(rustix::io::Errno::INTR) => continue,
                Err(rustix::io::Errno::WOULDBLOCK) => {
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
                Err(errno) => return Err(write_failed(path, &std::io::Error::from(errno))),
            }
        }

        Ok(Self {
            _file: file,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

// The lock is released when the file is closed, which Drop does for us. The
// file is deliberately not removed: unlinking it would let a second process
// create a fresh one and lock that instead, so both would believe they held the
// workspace.
