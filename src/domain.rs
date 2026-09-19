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

mod operation;

use serde::{Deserialize, Serialize};

pub use self::operation::{OpSet, Operation};
use crate::hash::ContentHash;
use crate::paths::RelPath;

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
}
