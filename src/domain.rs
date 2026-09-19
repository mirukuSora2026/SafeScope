//! The core model of a file change.
//!
//! Implementing create, replace, move and trash separately would produce four
//! copies of the recovery logic. Expressed as a **set of states before** and a
//! **set of states after**, applying, verifying, undoing and classifying a crash
//! each collapse into a single function.
//!
//! | Operation | Before                           | After                            |
//! |-----------|----------------------------------|----------------------------------|
//! | `create`  | `[(p, Absent)]`                  | `[(p, Present(h))]`              |
//! | `replace` | `[(p, Present(h0))]`             | `[(p, Present(h1))]`             |
//! | `trash`   | `[(p, Present(h0))]`             | `[(p, Absent)]`                  |
//! | `move`    | `[(a, Present(h)), (b, Absent)]` | `[(a, Absent), (b, Present(h))]` |
//!
//! Undo is the same [`Transition`] with the two sides swapped.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::hash::ContentHash;
use crate::paths::RelPath;

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

/// The state of one path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum FileState {
    /// Nothing exists at this path.
    Absent,
    /// A regular file exists, identified by content hash and length.
    ///
    /// Modification time is deliberately excluded: treating a touched-but-identical
    /// file as changed would block a perfectly good undo.
    Present { hash: ContentHash, len: u64 },
}

impl FileState {
    pub const fn present(hash: ContentHash, len: u64) -> Self {
        FileState::Present { hash, len }
    }

    pub const fn exists(&self) -> bool {
        matches!(self, FileState::Present { .. })
    }

    pub const fn hash(&self) -> Option<&ContentHash> {
        match self {
            FileState::Present { hash, .. } => Some(hash),
            FileState::Absent => None,
        }
    }
}

/// A path together with its state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathState {
    pub path: RelPath,
    pub state: FileState,
}

impl PathState {
    pub const fn new(path: RelPath, state: FileState) -> Self {
        Self { path, state }
    }

    pub const fn absent(path: RelPath) -> Self {
        Self {
            path,
            state: FileState::Absent,
        }
    }

    pub const fn present(path: RelPath, hash: ContentHash, len: u64) -> Self {
        Self {
            path,
            state: FileState::present(hash, len),
        }
    }
}

/// Which side of a transition is meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Before,
    After,
}

/// How observed state relates to a plan. The input to crash recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observation {
    /// Exactly the pre-operation state: the operation definitely did not run.
    MatchesBefore,
    /// Exactly the post-operation state: it already ran.
    MatchesAfter,
    /// Some paths match one side and some the other; state must be compared.
    Partial,
    /// Neither side matches: a conflict.
    Divergent,
}

/// One file operation expressed as a state transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    operation: Operation,
    before: Vec<PathState>,
    after: Vec<PathState>,
}

impl Transition {
    pub fn create(path: RelPath, hash: ContentHash, len: u64) -> Self {
        Self {
            operation: Operation::Create,
            before: vec![PathState::absent(path.clone())],
            after: vec![PathState::present(path, hash, len)],
        }
    }

    pub fn replace(path: RelPath, old: (ContentHash, u64), new: (ContentHash, u64)) -> Self {
        Self {
            operation: Operation::Replace,
            before: vec![PathState::present(path.clone(), old.0, old.1)],
            after: vec![PathState::present(path, new.0, new.1)],
        }
    }

    pub fn trash(path: RelPath, hash: ContentHash, len: u64) -> Self {
        Self {
            operation: Operation::Trash,
            before: vec![PathState::present(path.clone(), hash, len)],
            after: vec![PathState::absent(path)],
        }
    }

    pub fn rename(from: RelPath, to: RelPath, hash: ContentHash, len: u64) -> Self {
        Self {
            operation: Operation::Move,
            before: vec![
                PathState::present(from.clone(), hash, len),
                PathState::absent(to.clone()),
            ],
            after: vec![PathState::absent(from), PathState::present(to, hash, len)],
        }
    }

    pub const fn operation(&self) -> Operation {
        self.operation
    }

    pub fn states(&self, phase: Phase) -> &[PathState] {
        match phase {
            Phase::Before => &self.before,
            Phase::After => &self.after,
        }
    }

    /// Every path this transition touches, deduplicated and ordered.
    ///
    /// A task's "changed paths" budget is the union of these sets. A move touches
    /// two paths and contributes both, so the number keeps meaning "how many
    /// paths were affected" — which is what makes it readable.
    pub fn touched_paths(&self) -> Vec<&RelPath> {
        let mut paths: Vec<&RelPath> = self
            .before
            .iter()
            .chain(&self.after)
            .map(|entry| &entry.path)
            .collect();
        paths.sort_unstable();
        paths.dedup();
        paths
    }

    /// What this single transition costs.
    pub fn budget_cost(&self) -> BudgetCost {
        BudgetCost {
            paths: self.touched_paths().len(),
            moves: usize::from(self.operation == Operation::Move),
            operations: 1,
        }
    }

    /// The transition that undoes this one.
    pub fn invert(&self) -> Transition {
        Transition {
            operation: self.operation.inverse(),
            before: self.after.clone(),
            after: self.before.clone(),
        }
    }

    /// The expected state of one path on the given side.
    pub fn expected(&self, phase: Phase, path: &RelPath) -> Option<&FileState> {
        self.states(phase)
            .iter()
            .find(|entry| &entry.path == path)
            .map(|entry| &entry.state)
    }

    /// Whether observed state matches the given side exactly.
    pub fn matches(&self, phase: Phase, observed: &[PathState]) -> bool {
        let expected = self.states(phase);
        expected.len() == observed.len()
            && expected.iter().all(|wanted| {
                observed
                    .iter()
                    .any(|seen| seen.path == wanted.path && seen.state == wanted.state)
            })
    }

    /// Classifies observed state for crash recovery.
    ///
    /// `observed` is expected to cover every path in [`Self::touched_paths`].
    pub fn classify(&self, observed: &[PathState]) -> Observation {
        if self.matches(Phase::Before, observed) {
            return Observation::MatchesBefore;
        }
        if self.matches(Phase::After, observed) {
            return Observation::MatchesAfter;
        }
        let any_before = observed
            .iter()
            .any(|seen| self.expected(Phase::Before, &seen.path) == Some(&seen.state));
        let any_after = observed
            .iter()
            .any(|seen| self.expected(Phase::After, &seen.path) == Some(&seen.state));
        if any_before && any_after {
            Observation::Partial
        } else {
            Observation::Divergent
        }
    }
}

/// What one operation consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BudgetCost {
    /// Distinct paths affected.
    pub paths: usize,
    /// Moves performed.
    pub moves: usize,
    /// Ordinary change operations performed.
    pub operations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> RelPath {
        RelPath::parse(text).unwrap()
    }

    fn hash(text: &str) -> ContentHash {
        ContentHash::of_bytes(text.as_bytes())
    }

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
    fn a_move_touches_two_paths() {
        let transition = Transition::rename(path("a.txt"), path("b.txt"), hash("x"), 1);
        assert_eq!(transition.touched_paths().len(), 2);

        let cost = transition.budget_cost();
        assert_eq!(cost.paths, 2);
        assert_eq!(cost.moves, 1);
        assert_eq!(cost.operations, 1);
    }

    #[test]
    fn replacing_in_place_touches_one_path() {
        let transition = Transition::replace(path("a.txt"), (hash("old"), 3), (hash("new"), 3));
        assert_eq!(transition.touched_paths().len(), 1);
        assert_eq!(transition.budget_cost().moves, 0);
    }

    #[test]
    fn undo_swaps_the_two_sides() {
        let create = Transition::create(path("a.txt"), hash("v1"), 2);
        let undo = create.invert();
        assert_eq!(undo.operation(), Operation::Trash);
        assert_eq!(undo.states(Phase::Before), create.states(Phase::After));
        assert_eq!(undo.states(Phase::After), create.states(Phase::Before));
        assert_eq!(
            undo.invert(),
            create,
            "inverting twice returns the original"
        );
    }

    #[test]
    fn undoing_a_move_moves_it_back() {
        let moved = Transition::rename(path("a.txt"), path("b.txt"), hash("x"), 1);
        let undo = moved.invert();
        assert_eq!(undo.operation(), Operation::Move);
        assert_eq!(
            undo.expected(Phase::Before, &path("b.txt")),
            Some(&FileState::present(hash("x"), 1))
        );
        assert_eq!(
            undo.expected(Phase::After, &path("a.txt")),
            Some(&FileState::present(hash("x"), 1))
        );
    }

    #[test]
    fn recognises_that_nothing_ran() {
        let transition = Transition::replace(path("a.txt"), (hash("old"), 3), (hash("new"), 3));
        let observed = vec![PathState::present(path("a.txt"), hash("old"), 3)];
        assert_eq!(transition.classify(&observed), Observation::MatchesBefore);
    }

    #[test]
    fn recognises_that_it_already_ran() {
        let transition = Transition::replace(path("a.txt"), (hash("old"), 3), (hash("new"), 3));
        let observed = vec![PathState::present(path("a.txt"), hash("new"), 3)];
        assert_eq!(transition.classify(&observed), Observation::MatchesAfter);
    }

    #[test]
    fn a_third_state_is_a_conflict() {
        let transition = Transition::replace(path("a.txt"), (hash("old"), 3), (hash("new"), 3));
        let observed = vec![PathState::present(path("a.txt"), hash("edited by hand"), 9)];
        assert_eq!(transition.classify(&observed), Observation::Divergent);
    }

    #[test]
    fn a_half_finished_move_needs_comparison() {
        let transition = Transition::rename(path("a.txt"), path("b.txt"), hash("x"), 1);
        // The source is still there (before) and the destination exists (after).
        let observed = vec![
            PathState::present(path("a.txt"), hash("x"), 1),
            PathState::present(path("b.txt"), hash("x"), 1),
        ];
        assert_eq!(transition.classify(&observed), Observation::Partial);
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
