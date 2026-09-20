//! Where the drift baseline is kept.
//!
//! One file, rewritten atomically. There is no history: a baseline describes the
//! moment work started, and the only question ever asked of it is what the
//! workspace looked like then.
//!
//! A missing baseline is not an error. Drift detection is something a workspace
//! opts into by starting a task, and a workspace that never did should say that
//! it has no baseline rather than that nothing drifted — those are different
//! answers and only one of them is true.

use std::path::PathBuf;

use crate::drift::Baseline;
use crate::error::Result;

use super::{StatePaths, corrupted, read_failed, write_atomically};

/// Reads and writes the workspace baseline.
#[derive(Debug, Clone)]
pub struct BaselineStore {
    path: PathBuf,
}

impl BaselineStore {
    pub fn new(paths: &StatePaths) -> Self {
        Self {
            path: paths.baseline(),
        }
    }

    /// The stored baseline, or `None` if none was ever taken.
    pub fn current(&self) -> Result<Option<Baseline>> {
        let raw = match std::fs::read(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(read_failed(&self.path, &error)),
        };
        serde_json::from_slice(&raw)
            .map(Some)
            .map_err(|error| corrupted(&self.path, error))
    }

    /// Replaces the baseline.
    pub fn store(&self, baseline: &Baseline) -> Result<()> {
        let encoded = serde_json::to_vec_pretty(baseline).expect("serialisable");
        write_atomically(&self.path, &encoded)
    }

    /// Takes a baseline only if there is not one already.
    ///
    /// Never overwrites. A baseline is the last moment the workspace was known
    /// to be accounted for, and replacing it adopts everything done since as
    /// the new starting point — which is how drift disappears without anybody
    /// deciding it should. Replacing one is `accept`, and a person asks for it.
    pub fn ensure(&self, root: &std::path::Path) -> Result<()> {
        if self.current()?.is_some() {
            return Ok(());
        }
        self.store(&Baseline::capture(root))
    }

    /// Forgets the baseline, so a workspace stops claiming one it no longer has.
    pub fn clear(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(read_failed(&self.path, &error)),
        }
    }
}
