//! Temporary approvals, kept where a second process can see them.
//!
//! Grants have to outlive the process that obtained them. An approval typed at a
//! terminal is issued by one process and spent by another — the MCP server —
//! and an in-memory list would mean the two could never see the same one.
//!
//! Writing a grant is not a file change, so it does not take the workspace lock.
//! Issuing an approval while a server is running is exactly the case the
//! terminal path exists for; requiring the lock would make that impossible.
//!
//! Each grant is its own file, named by its id. Appending to a shared file would
//! mean a torn write could lose or corrupt every other grant beside it.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::Result;
use crate::ids::TaskId;
use crate::policy::{Grant, PolicyVersion};

use super::{StatePaths, corrupted, read_failed, write_atomically};

/// Reads and writes temporary approvals.
#[derive(Debug, Clone)]
pub struct GrantStore {
    directory: PathBuf,
}

impl GrantStore {
    pub fn new(paths: &StatePaths) -> Self {
        Self {
            directory: paths.root().join("grants"),
        }
    }

    /// Stores a grant, replacing any earlier version of it.
    pub fn issue(&self, grant: &Grant) -> Result<()> {
        let encoded = serde_json::to_vec_pretty(grant).expect("a grant is always serialisable");
        write_atomically(&self.path_of(grant), &encoded)
    }

    /// Every grant still usable by this task against this policy version.
    ///
    /// Expired and spent grants are skipped rather than deleted: a person asking
    /// what was approved is asking about what happened, not about what is still
    /// live.
    pub fn usable(
        &self,
        task: TaskId,
        policy_version: PolicyVersion,
        now: SystemTime,
    ) -> Result<Vec<Grant>> {
        Ok(self
            .all()?
            .into_iter()
            .filter(|grant| grant.is_usable(task, policy_version, now))
            .collect())
    }

    /// Every grant on record, whatever became of it.
    pub fn all(&self) -> Result<Vec<Grant>> {
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(read_failed(&self.directory, &error)),
        };

        let mut grants = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let text = fs::read_to_string(&path).map_err(|error| read_failed(&path, &error))?;
            // A grant that cannot be read is reported, never skipped: silently
            // ignoring one would turn a damaged store into a quieter one.
            grants.push(serde_json::from_str(&text).map_err(|error| corrupted(&path, error))?);
        }
        Ok(grants)
    }

    /// Records that a grant has been spent.
    pub fn consume(&self, grant: &Grant) -> Result<()> {
        self.issue(grant)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    fn path_of(&self, grant: &Grant) -> PathBuf {
        self.directory
            .join(format!("{}.json", grant.id.as_uuid().simple()))
    }
}
