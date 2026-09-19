# SafeScope

A Rust engine that keeps file changes inside an approved scope and budget, and
records what it changed so the change can be reviewed and undone. It ships as a
Claude Code plugin (skills, hooks, an MCP server) around that engine.

**SafeScope is not a sandbox.** It guarantees that changes made *through the
engine* carry a complete chain of scope, budget and recovery data. Changes made
around it — an arbitrary shell command, for instance — are not recorded, and the
status output must say so rather than implying coverage it does not have. Never
write documentation or messages that overstate this.

## Invariants

Every module upholds these. A change that weakens one is wrong even if it passes
the tests.

| #  | Invariant |
|----|-----------|
| I1 | The intent record is durable before any file changes |
| I2 | Recovery data is stored and verified before a destructive change |
| I3 | An operation runs only if observed state still matches the plan |
| I4 | Path decisions and execution share one directory handle |
| I5 | Budget is reserved before execution and released only if nothing ran |
| I6 | AI input can never serve as evidence of approval |

## Conventions

- **Identifiers and doc comments are English.** Rust convention, and the crate is
  intended to be readable by people who do not read Korean.
- **Every string a person can see goes through `src/dataformatting.rs`**, as a
  typed `Msg` variant translated in `src/dataformatting/{en,ko,zh,ja,ru}.rs`.
  Never `format!` user-facing text inline. Adding a `Msg` variant fails to
  compile until all five languages are updated — that is intentional.
- **Error codes are not translated.** They are a machine contract for JSON output
  and MCP responses. The message beside a code is translated; the code is not.
- **A denial is not a fault.** Use `Denial` when a rule refused, `Fault` when the
  engine or environment failed. Collapsing the two leaves callers unable to tell
  whether a retry is worth anything, and an AI client will loop.
- **Give denials a hint.** `Denial::with_hint` is what stops a client repeating a
  request that can never succeed.
- **Paths are rejected, not normalised.** `RelPath::parse` refuses `..` rather
  than resolving it. Never add a normalisation pass.
- **`RelPath` is the only way a path enters the engine.** Do not add a function
  that takes a path as `&str`.
- Test names are English `snake_case` sentences describing the behaviour, not the
  function under test.
- Files stay under 500 lines.

## Layout

```
src/
├─ lib.rs              crate root, invariant table, re-exports
├─ main.rs             the safescope executable (CLI lands in M1)
├─ dataformatting.rs   Language, Msg catalogue, byte/usage formatting
│  └─ en|ko|zh|ja|ru.rs
├─ error.rs            ErrorCode, Denial, Fault, ErrorReport
├─ paths.rs            RelPath — the validated relative path type
├─ hash.rs             ContentHash (BLAKE3), content addressing
├─ ids.rs              typed identifiers (TaskId, PlanId, GrantId, …)
├─ domain.rs           FileState, PathState, Transition, Observation
│  └─ operation.rs     Operation, OpSet
└─ fault.rs            crash injection points (feature `fault-injection`)
```

Modules still to come: `policy`, `path_guard`, `budget`, `planner`, `executor`,
`snapshot`, `journal`, `recovery`, `approval`, `platform`, `adapters`.

## Commands

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --features fault-injection
```

Crash-recovery tests run a child process with `SAFESCOPE_FAULT=<point>` under the
`fault-injection` feature and then assert on what recovery concludes.
