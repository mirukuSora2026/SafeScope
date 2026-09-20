//! SafeScope — an engine that keeps file changes inside an approved scope and
//! budget, and keeps a record of what it changed so the change can be reviewed
//! and undone.
//!
//! # This is not a sandbox
//!
//! SafeScope guarantees that **changes made through this engine** carry a
//! complete chain of scope, budget and recovery data. Changes made around it,
//! such as an arbitrary shell command, are not recorded — and the status screen
//! says so rather than implying coverage it does not have.
//!
//! # Invariants the whole engine upholds
//!
//! | # | Invariant |
//! |---|-----------|
//! | I1 | The intent record is durable before any file changes |
//! | I2 | Recovery data is stored and verified before a destructive change |
//! | I3 | An operation runs only if observed state still matches the plan |
//! | I4 | Path decisions and execution share one directory handle |
//! | I5 | Budget is reserved before execution and released only if nothing ran |
//! | I6 | AI input can never serve as evidence of approval |

pub mod budget;
pub mod cli;
pub mod dataformatting;
pub mod domain;
pub mod error;
pub mod executor;
pub mod fault;
pub mod hash;
pub mod ids;
pub mod inspect;
pub mod journal;
pub mod mcp;
pub mod path_guard;
pub mod paths;
pub mod planner;
pub mod platform;
pub mod policy;
pub mod recovery;
pub mod registry;
pub mod session;
pub mod store;
pub mod undo;

pub use dataformatting::{Language, Msg};
pub use domain::{
    BudgetCost, FileState, Observation, OpSet, Operation, PathState, Phase, Transition,
};
pub use error::{Denial, Error, ErrorCode, ErrorKind, ErrorReport, Fault, Result};
pub use hash::ContentHash;
pub use ids::{GrantId, OperationId, PlanId, RequestId, ReservationId, TaskId, WorkspaceId};
pub use path_guard::{Resolved, Workspace};
pub use paths::RelPath;
