//! Crash injection points.
//!
//! Testing "killed between the prepare, apply and commit records" automatically
//! requires the abort points to already be in the executor. They cannot be
//! retrofitted, which is why this module exists before the executor does.
//!
//! In a default build [`check`] compiles to nothing. Test binaries are built with
//! `--features fault-injection`:
//!
//! ```text
//! SAFESCOPE_FAULT=after_rename_before_commit \
//!   cargo run --features fault-injection -- apply --plan <id>
//! ```

use std::fmt;

/// A point at which the executor can be made to die.
///
/// Each name states what has just happened, so a recovery test's expected state
/// is readable from the name alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaultPoint {
    /// Straight after the plan record. No file has been touched.
    AfterPreparedRecord,
    /// After the snapshot is durable, before the original is removed.
    AfterSnapshotBeforeUnlink,
    /// After the record that says work is starting. Files are still untouched.
    AfterApplyingRecord,
    /// After the temporary file is written, before the atomic replace.
    AfterTemporaryWriteBeforeRename,
    /// After the file actually changed, before the commit record. The hardest window.
    AfterRenameBeforeCommit,
    /// Immediately before the budget is confirmed.
    BeforeBudgetCommit,
}

impl FaultPoint {
    pub const ALL: [FaultPoint; 6] = [
        FaultPoint::AfterPreparedRecord,
        FaultPoint::AfterSnapshotBeforeUnlink,
        FaultPoint::AfterApplyingRecord,
        FaultPoint::AfterTemporaryWriteBeforeRename,
        FaultPoint::AfterRenameBeforeCommit,
        FaultPoint::BeforeBudgetCommit,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            FaultPoint::AfterPreparedRecord => "after_prepared_record",
            FaultPoint::AfterSnapshotBeforeUnlink => "after_snapshot_before_unlink",
            FaultPoint::AfterApplyingRecord => "after_applying_record",
            FaultPoint::AfterTemporaryWriteBeforeRename => "after_temporary_write_before_rename",
            FaultPoint::AfterRenameBeforeCommit => "after_rename_before_commit",
            FaultPoint::BeforeBudgetCommit => "before_budget_commit",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        FaultPoint::ALL
            .into_iter()
            .find(|point| point.as_str() == text)
    }
}

impl fmt::Display for FaultPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Environment variable naming the armed fault point.
pub const ENV_VAR: &str = "SAFESCOPE_FAULT";

/// Kills the process if this point is armed.
///
/// `abort` rather than `exit`: running destructors and flushing buffers would
/// produce a state unlike a real power loss, sidestepping the very window under
/// test.
#[cfg(feature = "fault-injection")]
pub fn check(point: FaultPoint) {
    use std::sync::OnceLock;

    use crate::dataformatting::Msg;

    static ARMED: OnceLock<Option<FaultPoint>> = OnceLock::new();
    let armed = ARMED.get_or_init(|| {
        let raw = std::env::var(ENV_VAR).ok()?;
        let parsed = FaultPoint::parse(raw.trim());
        if parsed.is_none() {
            eprintln!(
                "{}",
                Msg::FaultUnknownValue {
                    variable: ENV_VAR.to_owned(),
                    value: raw
                }
            );
        }
        parsed
    });

    if *armed == Some(point) {
        eprintln!(
            "{}",
            Msg::FaultAborting {
                point: point.to_string()
            }
        );
        std::process::abort();
    }
}

/// Compiles away in a release build.
#[cfg(not(feature = "fault-injection"))]
#[inline(always)]
pub fn check(_point: FaultPoint) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for point in FaultPoint::ALL {
            assert_eq!(FaultPoint::parse(point.as_str()), Some(point));
        }
        assert_eq!(FaultPoint::parse("nope"), None);
    }

    #[test]
    fn names_are_distinct() {
        let mut names: Vec<_> = FaultPoint::ALL.iter().map(|point| point.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(
            names.len(),
            before,
            "duplicate names would confuse recovery tests"
        );
    }

    #[test]
    fn unarmed_checks_are_a_no_op() {
        // Nothing happens in a default build, and in a fault-injection build
        // nothing happens unless the environment variable names this point.
        for point in FaultPoint::ALL {
            check(point);
        }
    }
}
