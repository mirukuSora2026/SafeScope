//! The operations the engine performs, and sets of them.
//!
//! Split from the transition model so each file stays readable; the two are
//! re-exported together from [`crate::domain`].

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A file operation the engine supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    /// Creates a file that does not exist.
    Create,
    /// Replaces the whole contents of an existing file.
    Replace,
    /// Moves a file within one filesystem. Never overwrites the destination.
    Move,
    /// Removes a file from the workspace once recovery data is stored.
    Trash,
}

impl Operation {
    pub const ALL: [Operation; 4] = [
        Operation::Create,
        Operation::Replace,
        Operation::Move,
        Operation::Trash,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Operation::Create => "create",
            Operation::Replace => "replace",
            Operation::Move => "move",
            Operation::Trash => "trash",
        }
    }

    /// Whether the new contents must be staged before the plan is stored.
    ///
    /// Fixing the payload at plan time is what lets `apply` take nothing but a
    /// plan id, and what lets crash recovery know the *after* hash in advance.
    pub const fn needs_payload(self) -> bool {
        matches!(self, Operation::Create | Operation::Replace)
    }

    /// Whether existing contents are lost. If so, a snapshot must exist before
    /// the operation runs.
    pub const fn destroys_content(self) -> bool {
        matches!(self, Operation::Replace | Operation::Trash)
    }

    /// The operation that undoes this one.
    pub const fn inverse(self) -> Operation {
        match self {
            Operation::Create => Operation::Trash,
            Operation::Trash => Operation::Create,
            Operation::Replace => Operation::Replace,
            Operation::Move => Operation::Move,
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Operation {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Operation::ALL
            .into_iter()
            .find(|operation| operation.as_str() == text)
            .ok_or_else(|| {
                format!("unknown operation {text:?}; expected create, replace, move or trash")
            })
    }
}

/// A set of operations, as carried by a policy rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpSet(u8);

impl OpSet {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn all() -> Self {
        Self(0b1111)
    }

    const fn bit(operation: Operation) -> u8 {
        match operation {
            Operation::Create => 1,
            Operation::Replace => 2,
            Operation::Move => 4,
            Operation::Trash => 8,
        }
    }

    pub const fn contains(self, operation: Operation) -> bool {
        self.0 & Self::bit(operation) != 0
    }

    #[must_use]
    pub const fn with(self, operation: Operation) -> Self {
        Self(self.0 | Self::bit(operation))
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = Operation> {
        Operation::ALL
            .into_iter()
            .filter(move |&operation| self.contains(operation))
    }
}

impl FromIterator<Operation> for OpSet {
    fn from_iter<I: IntoIterator<Item = Operation>>(iter: I) -> Self {
        iter.into_iter().fold(OpSet::empty(), OpSet::with)
    }
}

impl fmt::Display for OpSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<_> = self.iter().map(Operation::as_str).collect();
        f.write_str(&names.join(", "))
    }
}

impl Serialize for OpSet {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for OpSet {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Vec::<Operation>::deserialize(deserializer)?
            .into_iter()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_sets_round_trip() {
        let set: OpSet = [Operation::Create, Operation::Trash].into_iter().collect();
        assert!(set.contains(Operation::Create));
        assert!(!set.contains(Operation::Move));

        let json = serde_json::to_string(&set).unwrap();
        assert_eq!(json, r#"["create","trash"]"#);
        assert_eq!(serde_json::from_str::<OpSet>(&json).unwrap(), set);
    }

    #[test]
    fn operation_properties_are_correct() {
        assert!(Operation::Create.needs_payload());
        assert!(Operation::Replace.needs_payload());
        assert!(!Operation::Move.needs_payload());
        assert!(!Operation::Trash.needs_payload());

        assert!(Operation::Replace.destroys_content());
        assert!(Operation::Trash.destroys_content());
        assert!(!Operation::Create.destroys_content());
        assert!(
            !Operation::Move.destroys_content(),
            "a move preserves content"
        );
    }

    #[test]
    fn operation_names_round_trip() {
        for operation in Operation::ALL {
            assert_eq!(Operation::from_str(operation.as_str()).unwrap(), operation);
        }
        assert!(Operation::from_str("delete").is_err());
    }
}
