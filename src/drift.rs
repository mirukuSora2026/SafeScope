//! What changed in the workspace without going through the engine.
//!
//! SafeScope records the changes it makes. It cannot prevent a change made
//! around it — a shell command, a tool it never sees — and the status output has
//! always said so. Saying so is not the same as noticing, though: a workspace
//! where nothing happened and a workspace where three files were rewritten by a
//! shell command both reported the same thing, which is nothing.
//!
//! This closes that gap. A baseline is taken when work starts; comparing the
//! tree against it afterwards says which files changed outside the engine, and
//! therefore which ones have no snapshot and cannot be undone.
//!
//! **This detects, it does not prevent.** A file listed here is already changed
//! and its previous contents are already gone. The value is that it is named
//! rather than silently absent from the record.
//!
//! The comparison is derived entirely from durable data — the stored baseline
//! and the journal — so a crash between a change and its bookkeeping cannot
//! leave it reporting a file the engine itself wrote.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::domain::{FileState, PathState};
use crate::hash::ContentHash;
use crate::paths::RelPath;
use crate::registry;

/// How many files a survey will look at before it stops and says so.
///
/// A survey runs at the end of a turn, so it has to finish in about the time a
/// person will wait. Stopping and reporting the truncation is honest; hashing a
/// repository of a hundred thousand files while somebody waits is not.
pub const SCAN_LIMIT: usize = 20_000;

/// Directory names a survey never descends into.
///
/// `.safescope` is the engine's own configuration and `.git` is the version
/// control system's business. Neither is work an agent should be credited or
/// blamed for, and `.git` alone would otherwise dominate every scan.
const SKIPPED: [&str; 2] = [registry::CONFIG_DIR, ".git"];

/// The state of the workspace at the moment work started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub captured_at: SystemTime,
    /// Whether the capture hit [`SCAN_LIMIT`].
    ///
    /// A truncated baseline cannot support a claim that a file is new, so the
    /// survey downgrades what it reports rather than guessing.
    #[serde(default)]
    pub truncated: bool,
    pub entries: Vec<PathState>,
}

impl Baseline {
    /// Walks the workspace and records what is there.
    pub fn capture(root: &Path) -> Baseline {
        let mut entries = Vec::new();
        let mut truncated = false;
        walk(root, root, &mut entries, &mut truncated);
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Baseline {
            captured_at: SystemTime::now(),
            truncated,
            entries,
        }
    }

    fn states(&self) -> BTreeMap<&RelPath, &FileState> {
        self.entries
            .iter()
            .map(|entry| (&entry.path, &entry.state))
            .collect()
    }
}

/// What happened to one path outside the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// A file exists that was not there when the baseline was taken.
    Added,
    /// A file's contents differ from the baseline and from anything recorded.
    Modified,
    /// A file in the baseline is gone.
    Removed,
}

/// One path that changed without the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: RelPath,
    pub change: Change,
}

/// The result of comparing the workspace against its baseline.
#[derive(Debug, Clone, Default)]
pub struct Survey {
    /// Paths that changed outside the engine, sorted.
    pub entries: Vec<Entry>,
    /// How many files the survey looked at.
    pub scanned: usize,
    /// Whether either the baseline or this scan hit [`SCAN_LIMIT`].
    ///
    /// Reported rather than hidden: a truncated survey can say what it found,
    /// not that there is nothing else.
    pub truncated: bool,
}

impl Survey {
    pub fn is_clean(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn count(&self, change: Change) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.change == change)
            .count()
    }
}

/// Compares the workspace against its baseline.
///
/// `recorded` is every state the engine observed after a change it made. A file
/// matching one of them was put there by SafeScope whatever the baseline says,
/// which is what stops the engine's own work being reported as drift.
pub fn survey(root: &Path, baseline: &Baseline, recorded: &[PathState]) -> Survey {
    let mut current = Vec::new();
    let mut truncated = baseline.truncated;
    walk(root, root, &mut current, &mut truncated);
    let scanned = current.len();

    // Hashes the engine put at a path. Keyed by path as well as hash: the same
    // contents written to a different path is still somebody else's doing.
    let engine: HashSet<(&str, &ContentHash)> = recorded
        .iter()
        .filter_map(|state| state.state.hash().map(|hash| (state.path.as_str(), hash)))
        .collect();
    // Paths the engine removed, which are likewise not drift when absent.
    let emptied: HashSet<&str> = recorded
        .iter()
        .filter(|state| !state.state.exists())
        .map(|state| state.path.as_str())
        .collect();

    let before = baseline.states();
    let mut entries = Vec::new();

    for state in &current {
        let expected = before.get(&state.path);
        if expected.is_some_and(|expected| *expected == &state.state) {
            continue;
        }
        if state
            .state
            .hash()
            .is_some_and(|hash| engine.contains(&(state.path.as_str(), hash)))
        {
            continue;
        }
        // A truncated baseline cannot distinguish "new" from "never looked at",
        // so an unknown path is reported as modified rather than added.
        let change = match expected {
            Some(_) => Change::Modified,
            None if baseline.truncated => Change::Modified,
            None => Change::Added,
        };
        entries.push(Entry {
            path: state.path.clone(),
            change,
        });
    }

    let present: HashSet<&RelPath> = current.iter().map(|state| &state.path).collect();
    for entry in &baseline.entries {
        if present.contains(&entry.path)
            || !entry.state.exists()
            || emptied.contains(entry.path.as_str())
        {
            continue;
        }
        entries.push(Entry {
            path: entry.path.clone(),
            change: Change::Removed,
        });
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Survey {
        entries,
        scanned,
        truncated,
    }
}

/// Records every regular file under `directory`, depth first.
///
/// Symlinks are recorded as absent rather than followed. Following one would let
/// a link decide what the baseline covers, and the engine refuses to follow them
/// for the same reason.
fn walk(root: &Path, directory: &Path, out: &mut Vec<PathState>, truncated: &mut bool) {
    let Ok(listing) = std::fs::read_dir(directory) else {
        return;
    };
    let mut children: Vec<_> = listing.flatten().map(|entry| entry.path()).collect();
    children.sort();

    for child in children {
        if out.len() >= SCAN_LIMIT {
            *truncated = true;
            return;
        }
        let name = child.file_name().unwrap_or_default().to_string_lossy();
        if SKIPPED.contains(&name.as_ref()) {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&child) else {
            continue;
        };
        if metadata.is_dir() {
            walk(root, &child, out, truncated);
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let Ok(relative) = child.strip_prefix(root) else {
            continue;
        };
        let Ok(path) = RelPath::from_platform(relative) else {
            continue;
        };
        if path.is_engine_temporary() {
            continue;
        }
        let Ok(file) = std::fs::File::open(&child) else {
            continue;
        };
        let Ok((hash, len)) = ContentHash::of_reader(file) else {
            continue;
        };
        out.push(PathState::new(path, FileState::present(hash, len)));
    }
}
