//! The approved policy store.
//!
//! This is what makes "editing policy.toml grants nothing" true rather than
//! aspirational. The engine reads only from here; `policy.toml` is read by the
//! approval command and by nothing else.
//!
//! ```text
//! approved-policy/
//!   000001.json   a full snapshot of one approved version
//!   000002.json
//!   current       the version in force, as a number
//! ```
//!
//! Each snapshot records the hash of the source text it was approved from, so
//! the status output can tell a person their policy file has unapproved edits —
//! which is the situation where someone believes a change took effect and it has
//! not.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::hash::ContentHash;
use crate::policy::{NormalizedPolicy, PolicyVersion};

use super::{StatePaths, corrupted, read_failed, write_atomically};

/// One approved version of a policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovedPolicy {
    pub version: PolicyVersion,
    /// Hash of the policy file text this was approved from.
    pub source_hash: ContentHash,
    pub approved_at: SystemTime,
    pub policy: NormalizedPolicy,
}

/// Reads and writes approved policy versions.
#[derive(Debug, Clone)]
pub struct PolicyStore {
    directory: PathBuf,
}

impl PolicyStore {
    pub fn new(paths: &StatePaths) -> Self {
        Self {
            directory: paths.approved_policy(),
        }
    }

    /// The version currently in force, if any has been approved.
    pub fn current(&self) -> Result<Option<ApprovedPolicy>> {
        let Some(version) = self.current_version()? else {
            return Ok(None);
        };
        self.read(version).map(Some)
    }

    /// The version number in force, without reading the whole snapshot.
    pub fn current_version(&self) -> Result<Option<PolicyVersion>> {
        let pointer = self.pointer_path();
        let text = match fs::read_to_string(&pointer) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(read_failed(&pointer, &error)),
        };
        text.trim()
            .parse::<u64>()
            .map(|value| Some(PolicyVersion::new(value)))
            .map_err(|error| corrupted(&pointer, error))
    }

    /// Reads one approved version.
    pub fn read(&self, version: PolicyVersion) -> Result<ApprovedPolicy> {
        let path = self.version_path(version);
        let text = fs::read_to_string(&path).map_err(|error| read_failed(&path, &error))?;
        let approved: ApprovedPolicy =
            serde_json::from_str(&text).map_err(|error| corrupted(&path, error))?;
        Ok(approved)
    }

    /// Stores a newly approved policy and makes it the version in force.
    ///
    /// The snapshot is written before the pointer moves. A crash between the two
    /// leaves an unreferenced snapshot, which is harmless; the other order would
    /// leave the pointer naming a file that does not exist.
    pub fn approve(&self, policy: NormalizedPolicy, source_text: &str) -> Result<ApprovedPolicy> {
        let version = self
            .current_version()?
            .map_or(PolicyVersion::FIRST, PolicyVersion::next);

        let approved = ApprovedPolicy {
            version,
            source_hash: ContentHash::of_bytes(source_text.as_bytes()),
            approved_at: SystemTime::now(),
            policy,
        };

        let encoded = serde_json::to_vec_pretty(&approved)
            .expect("an approved policy is always serialisable");
        write_atomically(&self.version_path(version), &encoded)?;
        write_atomically(&self.pointer_path(), format!("{version}\n").as_bytes())?;

        Ok(approved)
    }

    /// Whether the policy file has been edited since it was approved.
    ///
    /// The situation this exists for: someone changes the policy, does not
    /// approve it, and believes the change is in force.
    pub fn matches_source(&self, approved: &ApprovedPolicy, source_text: &str) -> bool {
        approved.source_hash == ContentHash::of_bytes(source_text.as_bytes())
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    fn pointer_path(&self) -> PathBuf {
        self.directory.join("current")
    }

    fn version_path(&self, version: PolicyVersion) -> PathBuf {
        self.directory.join(format!("{:06}.json", version.get()))
    }
}
