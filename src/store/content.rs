//! Content-addressed blob storage.
//!
//! Two stores use it, for the same reason: both hold bytes the engine must be
//! able to produce again exactly.
//!
//! - `snapshots/` holds what a file contained before a destructive change, which
//!   is what undo restores.
//! - `staging/` holds the new contents a plan will write, fixed at planning time
//!   so `apply` takes nothing but a plan id and so recovery knows the *after*
//!   hash in advance.
//!
//! They are separate directories rather than one, because they are kept for
//! different lengths of time: a staged payload stops mattering once its plan is
//! applied or abandoned, while a snapshot has to outlive the operation it undoes.
//!
//! Through the snapshot store this upholds invariant I2:
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
use crate::retention::Reclaimed;

use super::{StatePaths, read_failed, write_atomically};

/// A content-addressed store rooted at one directory.
#[derive(Debug, Clone)]
pub struct ContentStore {
    directory: PathBuf,
}

impl ContentStore {
    /// What a file contained before a destructive change.
    pub fn snapshots(paths: &StatePaths) -> Self {
        Self {
            directory: paths.snapshots(),
        }
    }

    /// What a plan will write, held between planning and applying.
    pub fn staging(paths: &StatePaths) -> Self {
        Self {
            directory: paths.staging(),
        }
    }

    /// A store at an explicit directory.
    pub fn at(directory: PathBuf) -> Self {
        Self { directory }
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
            // Marked as written now. Content addressing means a second plan with
            // the same contents writes nothing, and the staging sweep decides by
            // age — so without this it could reclaim a payload a newer plan is
            // still holding. Best effort: a clock that refuses to be set is not
            // a reason to fail a store that already succeeded.
            self.mark_as_fresh(hash);
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

    /// Records that a blob is wanted again, for the sweep that decides by age.
    ///
    /// Unix only. Windows has no equivalent here yet, so a payload re-staged
    /// there keeps the age of its first write and a sweep may reclaim it early —
    /// which costs a refused apply and a re-plan, never a wrong result.
    #[cfg(unix)]
    fn mark_as_fresh(&self, hash: ContentHash) {
        use rustix::fs::{AtFlags, Timestamps, utimensat};

        let now = rustix::fs::Timespec {
            tv_sec: 0,
            tv_nsec: rustix::fs::UTIME_NOW,
        };
        let _ = utimensat(
            rustix::fs::CWD,
            self.path_of(hash),
            &Timestamps {
                last_access: now,
                last_modification: now,
            },
            AtFlags::empty(),
        );
    }

    #[cfg(not(unix))]
    fn mark_as_fresh(&self, _hash: ContentHash) {}

    /// Reads stored contents back, checking them against their own name.
    pub fn read(&self, hash: ContentHash) -> Result<Vec<u8>> {
        let path = self.path_of(hash);
        let contents = match fs::read(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Absent, not corrupt. The likeliest reason is that retention
                // reclaimed it, and saying so is the difference between "your
                // store is damaged" and "this is older than you said to keep".
                return Err(Error::Faulted(
                    Fault::new(
                        ErrorCode::SnapshotFailed,
                        Msg::SnapshotMissing { hash: hash.short() },
                    )
                    .with_hint(Msg::HintSnapshotMayHaveAged),
                ));
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

    /// Removes every blob whose hash is not in `keep`.
    ///
    /// A sweep rather than a delete beside each record, because blobs are
    /// content addressed and therefore shared: two operations that saw the same
    /// contents point at one file, and removing it with the first would take it
    /// from the second.
    ///
    /// A blob that cannot be measured or removed is left alone and not counted.
    /// Recovery data is the thing being protected here, so a failure to tidy is
    /// not a reason to lose any.
    pub fn retain(&self, keep: &std::collections::HashSet<ContentHash>) -> Result<Reclaimed> {
        self.sweep(keep, None)
    }

    /// Removes blobs that are neither named in `keep` nor written since `since`.
    ///
    /// For the staging store, where what is still wanted cannot be named in
    /// full: a plan that was never applied left no record of its payload, and
    /// only its age says whether any plan could still reach it.
    pub fn retain_since(
        &self,
        keep: &std::collections::HashSet<ContentHash>,
        since: std::time::SystemTime,
    ) -> Result<Reclaimed> {
        self.sweep(keep, Some(since))
    }

    fn sweep(
        &self,
        keep: &std::collections::HashSet<ContentHash>,
        since: Option<std::time::SystemTime>,
    ) -> Result<Reclaimed> {
        let mut reclaimed = Reclaimed::default();
        let shards = match fs::read_dir(&self.directory) {
            Ok(shards) => shards,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(reclaimed),
            Err(error) => return Err(read_failed(&self.directory, &error)),
        };

        for shard in shards.flatten() {
            let Ok(entries) = fs::read_dir(shard.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(text) = name.to_str() else {
                    continue;
                };
                // The file name is the whole hash; the shard is only a directory
                // to keep any one of them from growing too wide. A name this
                // store did not write is not this store's to remove.
                let Ok(hash) = ContentHash::from_hex(text) else {
                    continue;
                };
                if keep.contains(&hash) {
                    continue;
                }
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if !metadata.is_file() {
                    continue;
                }
                // Too young to be unreachable. A blob that cannot report its age
                // is kept, because an age that cannot be established is not an
                // age past the cutoff.
                if let Some(since) = since
                    && metadata.modified().is_ok_and(|written| written >= since)
                {
                    continue;
                }
                if fs::remove_file(entry.path()).is_ok() {
                    reclaimed.blobs += 1;
                    reclaimed.bytes += metadata.len();
                }
            }
        }
        Ok(reclaimed)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// `<root>/<shard>/<hex>`, sharded so one directory does not accumulate
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
