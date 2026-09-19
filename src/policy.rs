//! Policy: what may be changed, how, and within what limits.
//!
//! The engine never reads the editable `policy.toml` directly. It reads an
//! approved snapshot, so editing the file grants nothing until a person approves
//! it at a terminal.

pub mod defaults;
pub mod file;
pub mod matcher;
pub mod normalized;
pub mod protected;
pub mod validate;

pub use self::defaults::SCHEMA_VERSION;
pub use self::file::{PolicyDocument, PolicyFile};
pub use self::matcher::{CaseSensitivity, Pattern, PatternSet};
pub use self::normalized::{AllowEntry, BudgetLimits, CompiledPolicy, DenyEntry, NormalizedPolicy};
pub use self::protected::{ProtectedMatch, ProtectedPaths, ProtectedReason};
pub use self::validate::{Diagnostic, Severity, ValidationReport, validate};
