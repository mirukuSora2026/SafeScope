//! Type-distinct identifiers.
//!
//! If everything were a `Uuid`, passing a `PlanId` where a `TaskId` belongs would
//! compile. In this engine, crossing identifiers means spending another task's
//! budget or applying the wrong plan, so each one gets its own newtype.
//!
//! Display carries a prefix (`plan_3f2a…`) so a person can tell the kinds apart
//! on screen; serialisation is the bare UUID so the stored form stays simple.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! typed_id {
    ($(#[$meta:meta])* $name:ident, $prefix:literal) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Prefix shown to people.
            pub const PREFIX: &'static str = $prefix;

            /// Generates a new random identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub const fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }

            /// The last four characters, which a person types back to confirm.
            ///
            /// Terminal approval asks for these so that what was read on screen
            /// and what is being approved are demonstrably the same request.
            pub fn tail(&self) -> String {
                let simple = self.0.simple().to_string();
                simple[simple.len() - 4..].to_owned()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}_{}", $prefix, self.0.simple())
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            /// Accepts the prefixed and bare forms alike.
            fn from_str(text: &str) -> Result<Self, Self::Err> {
                let body = text.strip_prefix(concat!($prefix, "_")).unwrap_or(text);
                Ok(Self(Uuid::parse_str(body)?))
            }
        }
    };
}

typed_id!(
    /// A workspace. Stored in `.safescope/workspace-id` so it survives the
    /// project being moved.
    WorkspaceId, "ws"
);
typed_id!(
    /// The unit that shares an approved scope and budget. Its lifetime is
    /// independent of a Claude session.
    TaskId, "task"
);
typed_id!(
    /// A checked and stored change plan. `apply` takes nothing else.
    PlanId, "plan"
);
typed_id!(
    /// One file operation that was performed, or attempted.
    OperationId, "op"
);
typed_id!(
    /// A temporary approval: exact paths, a time to live, one use.
    GrantId, "grant"
);
typed_id!(
    /// The caller's idempotency key, distinguishing a resend from a new request.
    RequestId, "req"
);
typed_id!(
    /// Budget held before an operation runs.
    ReservationId, "rsv"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_carries_the_prefix() {
        assert!(PlanId::new().to_string().starts_with("plan_"));
        assert!(TaskId::new().to_string().starts_with("task_"));
    }

    #[test]
    fn parses_prefixed_and_bare_forms() {
        let id = TaskId::new();
        assert_eq!(TaskId::from_str(&id.to_string()).unwrap(), id);
        assert_eq!(TaskId::from_str(&id.as_uuid().to_string()).unwrap(), id);
    }

    #[test]
    fn serialises_as_a_bare_uuid() {
        let id = GrantId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert!(
            !json.contains("grant_"),
            "the stored form stays a plain UUID"
        );
        assert_eq!(serde_json::from_str::<GrantId>(&json).unwrap(), id);
    }

    #[test]
    fn confirmation_tail_is_four_characters() {
        let id = RequestId::new();
        let tail = id.tail();
        assert_eq!(tail.len(), 4);
        assert!(id.as_uuid().simple().to_string().ends_with(&tail));
    }
}
