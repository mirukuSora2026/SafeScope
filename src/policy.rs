//! Policy: what may be changed, how, and within what limits.
//!
//! The engine never reads the editable `policy.toml` directly. It reads an
//! approved snapshot, so editing the file grants nothing until a person approves
//! it at a terminal.

pub mod file;
pub mod matcher;
pub mod protected;

pub use self::file::{PolicyDocument, PolicyFile, SCHEMA_VERSION};
pub use self::matcher::{CaseSensitivity, Pattern, PatternSet};
pub use self::protected::{ProtectedMatch, ProtectedPaths, ProtectedReason};
