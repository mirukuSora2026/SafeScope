//! Registering a project as a workspace.
//!
//! Registration writes two files into `.safescope/`:
//!
//! - `workspace-id`, a UUID that is the primary key for engine state
//! - `policy.toml`, a starter policy that permits nothing
//!
//! The UUID rather than the path is the key, because a project that is moved or
//! copied is still the same project; device and inode numbers are not stable
//! enough to rely on, since a checkout or a restore changes both.
//!
//! The starter policy grants nothing on purpose. A default that permitted
//! something would mean nobody reads the policy.

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr as _;

use crate::dataformatting::Msg;
use crate::error::{Denial, Error, ErrorCode, Result};
use crate::ids::WorkspaceId;
use crate::policy::normalized::starter_policy_text;
use crate::store::{StatePaths, read_failed, write_atomically, write_failed};

/// Directory holding the project-side SafeScope files.
pub const CONFIG_DIR: &str = ".safescope";
/// File naming the workspace, generated per checkout and not shared.
pub const ID_FILE: &str = "workspace-id";
/// The editable policy draft.
pub const POLICY_FILE: &str = "policy.toml";

/// A registered workspace.
#[derive(Debug, Clone)]
pub struct Registration {
    id: WorkspaceId,
    root: PathBuf,
}

impl Registration {
    pub const fn id(&self) -> WorkspaceId {
        self.id
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_dir(&self) -> PathBuf {
        self.root.join(CONFIG_DIR)
    }

    /// The editable policy draft. Read by the approval command and nothing else.
    pub fn policy_path(&self) -> PathBuf {
        self.config_dir().join(POLICY_FILE)
    }

    /// Where engine state for this workspace lives.
    pub fn state_paths(&self) -> Result<StatePaths> {
        StatePaths::for_workspace(self.id, &self.root)
    }

    /// Reads the policy draft.
    pub fn read_policy_text(&self) -> Result<String> {
        let path = self.policy_path();
        fs::read_to_string(&path).map_err(|error| read_failed(&path, &error))
    }
}

/// Registers a project, failing if it already is one.
///
/// State placement is checked before anything is written, so a project is never
/// left half-registered because its state directory turned out to be unusable.
pub fn init(root: &Path) -> Result<Registration> {
    let config = root.join(CONFIG_DIR);
    let id_path = config.join(ID_FILE);

    if id_path.exists() {
        return Err(Error::Denied(Denial::new(
            ErrorCode::WorkspaceAlreadyRegistered,
            Msg::WorkspaceAlreadyRegistered {
                root: root.display().to_string(),
            },
        )));
    }

    let id = WorkspaceId::new();
    StatePaths::for_workspace(id, root)?.create()?;

    fs::create_dir_all(&config).map_err(|error| write_failed(&config, &error))?;
    write_atomically(&id_path, format!("{}\n", id.as_uuid().simple()).as_bytes())?;

    // An existing policy is left alone: someone may have written one before
    // running init, and overwriting it would throw away their work.
    let policy_path = config.join(POLICY_FILE);
    if !policy_path.exists() {
        write_atomically(&policy_path, starter_policy_text().as_bytes())?;
    }

    Ok(Registration {
        id,
        root: root.to_path_buf(),
    })
}

/// Loads an existing registration.
pub fn load(root: &Path) -> Result<Registration> {
    let id_path = root.join(CONFIG_DIR).join(ID_FILE);
    let text = match fs::read_to_string(&id_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::Denied(
                Denial::new(
                    ErrorCode::WorkspaceNotRegistered,
                    Msg::WorkspaceNotRegistered {
                        root: root.display().to_string(),
                    },
                )
                .with_hint(Msg::HintRunInitFirst),
            ));
        }
        Err(error) => return Err(read_failed(&id_path, &error)),
    };

    let id = WorkspaceId::from_str(text.trim()).map_err(|_| {
        // Not repaired automatically: a new identity would orphan every snapshot
        // and journal record belonging to the old one.
        Error::Denied(Denial::new(
            ErrorCode::WorkspaceNotRegistered,
            Msg::WorkspaceIdCorrupted {
                path: id_path.display().to_string(),
            },
        ))
    })?;

    Ok(Registration {
        id,
        root: root.to_path_buf(),
    })
}
