//! Recovery data, addressed by content.
//!
//! This is the store that makes undo possible, so it upholds invariant I2:
//! **recovery data is stored and verified before a destructive change**. Storing
//! is not enough on its own — a write that returned success and a file that
//! reads back correctly are different claims, and the second is the one undo
//! actually depends on. Every store therefore reads the file back and re-hashes
//! it before reporting success.
//!
//! That costs a read of a file that was just written, which usually comes from
//! the page cache. It buys the difference between "we wrote a snapshot" and "the
//! snapshot is there", which is the whole value of the store.
//!
//! Content addressing means the same contents are stored once no matter how many
//! operations touch them, and it makes verification trivial: the name is the
//! answer the contents must hash to.

use std::fs;
use std::path::{Path, PathBuf};

use crate::dataformatting::Msg;
use crate::error::{Error, ErrorCode, Fault, Result};
use crate::hash::ContentHash;

use super::{StatePaths, read_failed, write_atomically};

/// The content-addressed store under `snapshots/`.
#[derive(Debug, Clone)]
pub struct SnapshotStore {
    directory: PathBuf,
}

impl SnapshotStore {
    pub fn new(paths: &StatePaths) -> Self {
        Self {
            directory: paths.snapshots(),
        }
    }

    /// Stores contents and returns the hash they are filed under.
    ///
    /// Storing the same contents twice is free the second time — but only after
    /// the existing copy has been verified. Trusting a stored file because its
    /// name exists would mean a corrupted snapshot silently satisfies the check
    /// that is supposed to guarantee undo.
    pub fn store(&self, contents: &[u8]) -> Result<ContentHash> {
        let hash = ContentHash::of_bytes(contents);

        if self.verify(hash).unwrap_or(false) {
            return Ok(hash);
        }

        let path = self.path_of(hash);
        write_atomically(&path, contents).map_err(|error| store_failed(&error))?;

        // I2: the claim is not "it was written" but "it reads back correctly".
        if !self.verify(hash)? {
            return Err(Error::Faulted(Fault::new(
                ErrorCode::SnapshotFailed,
                Msg::SnapshotVerificationFailed { hash: hash.short() },
            )));
        }
        Ok(hash)
    }

    /// Reads stored contents back, checking them against their own name.
    pub fn read(&self, hash: ContentHash) -> Result<Vec<u8>> {
        let path = self.path_of(hash);
        let contents = match fs::read(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::Faulted(Fault::new(
                    ErrorCode::SnapshotFailed,
                    Msg::SnapshotMissing { hash: hash.short() },
                )));
            }
            Err(error) => return Err(read_failed(&path, &error)),
        };

        // A snapshot that no longer hashes to its own name is not recovery data,
        // whatever it is. Handing it back would undo a change into corruption.
        if ContentHash::of_bytes(&contents) != hash {
            return Err(Error::Faulted(Fault::new(
                ErrorCode::SnapshotFailed,
                Msg::SnapshotVerificationFailed { hash: hash.short() },
            )));
        }
        Ok(contents)
    }

    /// Whether stored contents are present and still hash to their name.
    pub fn verify(&self, hash: ContentHash) -> Result<bool> {
        let path = self.path_of(hash);
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(read_failed(&path, &error)),
        };
        let (found, _) =
            ContentHash::of_reader(file).map_err(|error| read_failed(&path, &error))?;
        Ok(found == hash)
    }

    /// Whether a name is present, without reading the contents.
    ///
    /// Only for reporting. Anything that undo depends on uses [`Self::verify`],
    /// because presence is not integrity.
    pub fn contains(&self, hash: ContentHash) -> bool {
        self.path_of(hash).is_file()
    }

    /// Total bytes held, for the recovery-storage budget.
    ///
    /// Walks the store. Callers do this at planning time, not per operation.
    pub fn usage_bytes(&self) -> Result<u64> {
        let mut total = 0u64;
        let shards = match fs::read_dir(&self.directory) {
            Ok(shards) => shards,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(read_failed(&self.directory, &error)),
        };
        for shard in shards.flatten() {
            let Ok(entries) = fs::read_dir(shard.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata()
                    && metadata.is_file()
                {
                    total += metadata.len();
                }
            }
        }
        Ok(total)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// `snapshots/<shard>/<hex>`, sharded so one directory does not accumulate
    /// tens of thousands of entries.
    fn path_of(&self, hash: ContentHash) -> PathBuf {
        self.directory.join(hash.shard()).join(hash.to_hex())
    }
}

fn store_failed(error: &Error) -> Error {
    Error::Faulted(Fault::new(
        ErrorCode::SnapshotFailed,
        Msg::SnapshotStoreFailed {
            reason: error.report().message,
        },
    ))
}
