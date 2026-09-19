//! Which task a workspace is currently working on.
//!
//! A task outlives a Claude session. Restarting the client should not hand
//! somebody a fresh budget — the limit is about the work, not about how many
//! times the tool was opened — so the current task is stored beside the journal
//! and picked up again.
//!
//! Starting a new task is therefore something a person decides, not something
//! that happens by reconnecting.

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr as _;

use crate::error::Result;
use crate::ids::TaskId;

use super::{StatePaths, corrupted, read_failed, write_atomically};

/// Reads and writes the workspace's current task.
#[derive(Debug, Clone)]
pub struct TaskStore {
    path: PathBuf,
}

impl TaskStore {
    pub fn new(paths: &StatePaths) -> Self {
        Self {
            path: paths.root().join("active-task"),
        }
    }

    /// The task in progress, if there is one.
    pub fn current(&self) -> Result<Option<TaskId>> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(read_failed(&self.path, &error)),
        };
        TaskId::from_str(text.trim())
            .map(Some)
            .map_err(|error| corrupted(&self.path, error))
    }

    /// The task in progress, starting one if there is none.
    pub fn current_or_start(&self) -> Result<TaskId> {
        match self.current()? {
            Some(task) => Ok(task),
            None => self.start(),
        }
    }

    /// Begins a new task, which resets what the budget has to spend.
    pub fn start(&self) -> Result<TaskId> {
        let task = TaskId::new();
        write_atomically(
            &self.path,
            format!("{}\n", task.as_uuid().simple()).as_bytes(),
        )?;
        Ok(task)
    }

    /// Ends the current task. The journal keeps its history.
    pub fn finish(&self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(read_failed(&self.path, &error)),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
