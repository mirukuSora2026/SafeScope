//! Default values for the policy file.
//!
//! Kept beside the schema rather than inside it so the numbers a person needs to
//! reason about — what happens when a field is left out — sit in one short file.

/// The only schema version this build understands.
pub const SCHEMA_VERSION: u32 = 1;

/// Default budget limits.
///
/// `max_changed_paths` is 8 rather than a tighter number because a move affects
/// two paths: at 5, three renames would exhaust the budget, and renaming is not
/// the sprawl this limit exists to catch.
pub const DEFAULT_MAX_CHANGED_PATHS: u64 = 8;
pub const DEFAULT_MAX_MOVES: u64 = 3;
pub const DEFAULT_MAX_OPERATIONS: u64 = 20;
pub const DEFAULT_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
pub const DEFAULT_MAX_SNAPSHOT_BYTES: u64 = 128 * 1024 * 1024;
pub const DEFAULT_WARN_AT_RATIO: f64 = 0.8;
pub const DEFAULT_MAX_ELICITATIONS_PER_TASK: u32 = 3;
pub const DEFAULT_GRANT_TTL_MINUTES: u64 = 30;
pub const DEFAULT_RETAIN_CLOSED_TASK_DAYS: u32 = 14;
pub const DEFAULT_WORKSPACE_WRITER_LIMIT: u32 = 1;

pub(super) const fn default_max_changed_paths() -> u64 {
    DEFAULT_MAX_CHANGED_PATHS
}
pub(super) const fn default_max_moves() -> u64 {
    DEFAULT_MAX_MOVES
}
pub(super) const fn default_max_operations() -> u64 {
    DEFAULT_MAX_OPERATIONS
}
pub(super) const fn default_max_file_bytes() -> u64 {
    DEFAULT_MAX_FILE_BYTES
}
pub(super) const fn default_max_snapshot_bytes() -> u64 {
    DEFAULT_MAX_SNAPSHOT_BYTES
}
pub(super) const fn default_warn_at_ratio() -> f64 {
    DEFAULT_WARN_AT_RATIO
}
pub(super) const fn default_max_elicitations_per_task() -> u32 {
    DEFAULT_MAX_ELICITATIONS_PER_TASK
}
pub(super) const fn default_grant_ttl_minutes() -> u64 {
    DEFAULT_GRANT_TTL_MINUTES
}
pub(super) const fn default_retain_closed_task_days() -> u32 {
    DEFAULT_RETAIN_CLOSED_TASK_DAYS
}
pub(super) const fn default_workspace_writer_limit() -> u32 {
    DEFAULT_WORKSPACE_WRITER_LIMIT
}
pub(super) const fn default_true() -> bool {
    true
}
