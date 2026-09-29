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
- **Give errors a hint.** `with_hint` exists on both `Denial` and `Fault`; it is
  what stops a client repeating a request that can never succeed, and what tells
  a person what to do about one that might.
- **Paths are rejected, not normalised.** `RelPath::parse` refuses `..` rather
  than resolving it. Never add a normalisation pass.
- **`RelPath` is the only way a path enters the engine.** Do not add a function
  that takes a path as `&str`.
- Test names are English `snake_case` sentences describing the behaviour, not the
  function under test.
- Files stay under 500 lines. Moving a module's tests to `tests/` is usually the
  right way to get back under it, and often improves them.
- Column alignment goes through `dataformatting::pad`, not `{:<n}`. Format width
  counts characters, so a Korean or Japanese label leaves every later column
  ragged.
- Plugin manifests are checked against the host's documentation, never guessed
  at, and a test runs the argv each manifest names. A manifest whose binary never
  answers is a plugin that silently does nothing.
- Skills are instructions, not enforcement. Every check a skill describes is also
  made inside the engine, because a skill is advice a model may or may not follow.
- The hook is not the boundary. Per the host's documented behaviour a hook that
  exits with anything but 0 or 2 is non-blocking, and a disabled hook never runs,
  so every check it performs is also performed inside the engine. Never write a
  check that exists only in the hook.
- A question takes no lock. `Inspector` reads; `WriteSession` writes and holds
  the workspace lock. A status that failed whenever the MCP server was running
  would be a status nobody could ask for when it mattered.
- Report every check, not only the failures. Listing what went wrong reads as a
  clean bill of health for everything that was never looked at.
- Callers go through `WriteSession`, not through Planner and Executor directly.
  The session holds the workspace lock for its lifetime, which is what makes the
  budget's check-then-reserve sound, and it is the one place that knows the
  executor's stores must be the planner's stores.
- An approval obtained through the client is weaker evidence than one typed at a
  terminal, and everything says so: the grant records which it was, the status
  output counts them, a client that never declared the elicitation capability
  cannot be asked at all, and a run of requests escalates to a terminal. Never
  collapse the two into a boolean.
- Undo is not a new grant. It reverses what the engine did under an approved
  policy, so it skips scope evaluation (`Authority::Reversal`) and does not spend
  the change budget. Protected paths still apply. Checking either would trap
  people: a rule allowing `create` but not `trash` would let the engine make a
  file it then refuses to remove.
- Resolution opens without following, and never checks a name and then opens it.
  `cap-std` follows symlinks by default and resolves them itself before it opens
  anything, so a raw `O_NOFOLLOW` never reaches the syscall — the flag arrives
  after the link has been followed. Use `follow(FollowSymlinks::No)` for a file
  and `open_dir_nofollow` for a component, and take the metadata from the open
  handle. A check beside an open is two questions about a name that can change
  in between, and the thing described must be the thing that was read.
- Where safety rests on a dependency's behaviour, a test asserts that behaviour
  directly. `open_dir` following a symlinked component was invisible until a
  test asked it the question, because the check beside it hid the answer.
- A platform is supported when it has been run, not when it compiles. The first
  Linux run found every directory flush failing with EBADF, on code that had
  built cleanly for months. Say "compiles, never run" until CI says otherwise.
- A path the platform produced becomes a `RelPath` through
  `RelPath::from_platform`, never `to_string_lossy` and `parse`. On Windows the
  separator is `\`, `parse` refuses it, and a caller that skipped what failed
  to parse saw nothing below the top level — drift reported a clean workspace
  and the hook let every nested write through, on code that passed on Unix.
- A Win32 code is not an errno. `raw_os_error` holds one on Windows and the
  numbers overlap — 17 is EEXIST on Unix and ERROR_NOT_SAME_DEVICE there — so
  compare errno constants only under `cfg(unix)` and let Windows codes arrive
  as kinds.
- When a platform call fails for reasons the documentation does not explain,
  probe the candidates side by side on that platform rather than guessing one
  per CI round. Three rounds of reasoning about `SetFileInformationByHandle`
  were each wrong; one probe showed it refuses a `RootDirectory` its own
  documentation describes, and that the NT call beneath it does not.
- Refuse what the platform cannot do properly; never emulate it. A no-overwrite
  rename that falls back to checking first reopens the race the flag closes, and
  a cross-filesystem move done as copy-then-delete is not one operation.
- Never parse a shell command to decide whether it writes. `sed -i`, a redirect
  and a script are not reliably readable, and a check that looks like protection
  without being it is worse than none. Report the gap in the status output.
  Refusing a shell command under `mode = "allowlist"` is not an exception: it is
  refused for not being on a list, never for what it appears to do.
- Only the guard is a boundary; everything else is a check. A policy, a hook and
  an allowlist all depend on something agreeing to consult them, and the
  measurements say what happens when it does not. Never describe one of them as
  preventing anything — they refuse, which is different.
- The guard's sandbox profile names the resolved path as well as the given one.
  On macOS `/var` is a symlink to `/private/var`, so a profile naming only the
  first loads without error and protects nothing.
- Landlock has no deny rule, so the guard says "everything but the workspace" by
  granting read on `/` and write on the siblings of each step down to it. A path
  that cannot be read while that list is built is skipped, which loses write
  access rather than granting it: a rule that fails to be written fails closed.
- The Landlock ruleset is built before the fork and applied in `pre_exec`.
  Building it allocates and reads directories, neither of which is allowed after
  a fork; applying it any earlier would take the workspace from the engine,
  which is the one process that still has to be able to write there.
- Refuse by allowlist, never by deny list. Measured: given a list of forbidden
  tools an agent moves to one that is not on it, and the set of tools that can
  run a shell command is not knowable in advance. A deny list is a promise that
  cannot be kept.
- A baseline is never overwritten on the engine's own initiative. It records the
  last moment the workspace was known to be accounted for, so replacing it
  adopts everything done since — which is how drift disappears without anybody
  deciding it should. `safescope drift accept` is a person's decision.
- A plugin manifest is checked by running what the host would run, not what the
  test assembles. Reading `args` and building the argv by hand hid two separate
  manifests that the host silently ignored, both of which the tests called
  healthy.

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
├─ fault.rs            crash injection points (feature `fault-injection`)
├─ policy/             scope and budget rules
│  ├─ defaults.rs      what an omitted field means
│  ├─ file.rs          the TOML schema, parsed with line numbers
│  ├─ normalized.rs    the desugared form that gets approved and stored
│  ├─ validate.rs      checks run at approval time
│  ├─ matcher.rs       glob compilation
│  ├─ protected.rs     paths no rule can unlock
│  ├─ grant.rs         temporary approvals
│  └─ evaluate.rs      protected → deny → allow → grant → not covered
├─ planner.rs          request → checked plan; stages payload, takes snapshot
├─ executor.rs         applies a plan in the order the invariants require
├─ session.rs          WriteSession — holds the lock, assembles the engine
├─ inspect.rs          Inspector — reads a workspace without taking it
├─ budget.rs           how much a task may change (I5)
├─ undo.rs             reversing the last recorded operation
├─ recovery.rs         what happened when the engine stopped mid-operation
├─ journal.rs          the record of what was done and attempted (I1)
│  └─ record.rs        Stage, OperationRecord
├─ path_guard.rs       Workspace, Resolved — filesystem resolution (I4)
├─ platform.rs         atomic rename, staged writes, durable removal
├─ registry.rs         .safescope/ bootstrap and workspace identity
├─ store.rs            state layout, atomic writes
│  ├─ content.rs       content-addressed blobs: snapshots (I2) and payloads
│  ├─ grant_store.rs   temporary approvals, visible across processes
│  ├─ lock.rs          one writer at a time, per workspace
│  ├─ task_store.rs    which task the workspace is on
│  └─ policy_store.rs  approved policy versions
├─ mcp.rs              the MCP server Claude talks to
│  ├─ approval.rs     asking a person to widen the scope
│  └─ wire.rs         what the tools take and return
└─ cli.rs              init, policy, approve, check, status, history,
   │                   doctor, recover, undo, hook, mcp
   ├─ approve.rs      approving a policy
   ├─ grant.rs        approving a scope expansion at a terminal
   ├─ report.rs       status, history, doctor — read-only, no lock
   ├─ repair.rs      recover, undo — these take the lock
   ├─ check.rs
   └─ hook.rs         the PreToolUse and session hooks
```

The engine, the plugin and the command line are all in place.

```
plugin/                   the Claude Code package
├─ .claude-plugin/plugin.json
├─ .mcp.json              the safescope MCP server
├─ hooks/hooks.json       PreToolUse, SessionStart, Stop
├─ skills/                start, plan, status, history, undo, finish
└─ bin/safescope          built by scripts/build-plugin.sh, not checked in
```

## Commands

```bash
./scripts/verify.sh     # every gate, each reported separately
```

Run that rather than chaining the gates by hand. A `cargo clippy ... | tail`
reports the exit status of `tail`, which is how a clippy failure once got
committed.

Crash-recovery tests spawn `sfs-crash-harness` with `SAFESCOPE_FAULT=<point>`
under the `fault-injection` feature, abort it mid-operation, and then assert on
what recovery concludes. The harness binary is `required-features` gated, so it
never ships.

Tests that set `SAFESCOPE_DATA_DIR` must hold a lock while they set *and* use it.
It is process-wide and the suite runs in parallel, so without one a test reads
another's state directory — and only sometimes, which is the worst way for a test
to be wrong.
