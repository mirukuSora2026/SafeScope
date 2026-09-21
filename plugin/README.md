# SafeScope plugin

Keeps file changes inside an approved scope and budget, and records them so they
can be reviewed and undone.

## What it does not do

**SafeScope is not a sandbox.** It guarantees that changes made *through its own
tools* carry a complete chain of scope, budget and recovery data. A file written
by a shell command is not recorded and cannot be undone through it.

The status output says so, and so should you.

## Where this runs

| | engine | `safescope guard` | evidence |
|---|---|---|---|
| macOS (Apple silicon) | every gate, 403 tests | yes | run here |
| Linux (arm64) | every gate, 394 tests | no — refuses | run here |
| Windows (x86-64) | **compiles; never run** | no — refuses | cross-build only |

The first two rows were run, not inferred. Windows was not: the port compiles,
including every test, and no part of it has executed. Treat it as untested until
the `windows-latest` job in CI is green — the first time this was run on Linux
it turned out every durability guarantee had been silently unenforced there, and
that is what an unrun platform is worth.

The tests Linux does not run are the guard's own, `cfg`-gated to the platform
that has a sandbox. Windows additionally skips three symlink tests, because
creating a symlink there needs a privilege a test cannot assume.

### What Windows does differently

- **No-overwrite rename** is `SetFileInformationByHandle` with `FileRenameInfo`:
  relative to a directory handle, and refused by the kernel when the
  destination is taken. Not a check followed by a rename.
- **The single-writer lock** is `LockFileEx`, which refuses rather than waits
  and is released when the process dies.
- **A directory cannot be flushed.** Windows has no equivalent, so a crash
  immediately after a rename can lose that rename. It cannot produce a
  half-written file — contents are flushed before the rename as everywhere else
  — so what is lost is a completed operation, which recovery already classifies.
  This guarantee is weaker than on Unix.

`safescope guard` is macOS only, through `sandbox-exec`, and refuses elsewhere
rather than running a command unprotected. Everything else — the policy, the
hook, the allowlist, drift detection — is on every platform.

## Installing

```bash
./scripts/build-plugin.sh     # builds the engine into plugin/bin/
```

The binary is not checked in, so that step is not optional: the manifest names
`bin/safescope`, and a manifest whose binary is absent is a plugin that loads
and silently does nothing.

Then add the plugin directory in Claude Code. In the project:

```bash
safescope init
$EDITOR .safescope/policy.toml   # fill in the allow list
safescope policy approve          # must be typed at a terminal
```

Nothing may be changed until that last step is done by a person.

## Making it mean something

Installed as it is, SafeScope records what goes through it and declines what
falls outside the policy — but nothing stops Claude writing the same files
another way. Denying the built-in edit tools in the project's
`.claude/settings.json` narrows that:

```json
{
  "permissions": {
    "deny": ["Write", "Edit", "MultiEdit", "NotebookEdit"]
  }
}
```

### What this still does not buy

**There is no configuration that guarantees a complete record.** This was
measured, not assumed, by running real agent sessions against these projects:

- With `Write` and `Edit` denied, the agent wrote the files through `Bash`
  instead — `python3 -c "open(...).write(...)"`, `cat >`, `sed -i`. Three runs
  out of three. Nothing was recorded.
- With `Bash` denied as well, it wrote them through `Monitor`, which also runs
  shell commands. Two runs out of two. Nothing was recorded. Before settling on
  it, the same runs tried `Agent` and `Skill`, each of which would have handed
  the work to something holding its own tools.

Denying one way of running a command moves the work to another. The set of tools
that can run one depends on the host and the session, so no list of denials is
ever known to be complete. Anyone offering you one is describing a sandbox.

### Refusing by list instead, and noticing when that fails

Two things follow from the above, and neither is sufficient alone.

**`[enforcement] mode = "allowlist"`** inverts the question. Rather than naming
the tools to refuse, it names the ones to permit — reading, searching, and
SafeScope's own tools — and refuses everything else, including tools this build
has never heard of. That removes the gap a deny list has, at a price worth
stating plainly: the agent cannot run a shell command at all, so it cannot run
your tests either. The default is still `audit`, which judges the tools that
carry a path and says nothing about the rest.

Measured on the same project: with the allowlist in force the agent tried
`Edit`, `Bash` and `Agent`, was refused each time with the reason, and changed
nothing.

**`safescope guard -- <command>`** stops being a check and becomes a boundary.
The command runs under a kernel sandbox with every write to the workspace
denied, so a change outside the engine is impossible rather than refused, and
everything the command starts inherits it. The engine runs outside the sandbox
and the agent reaches it over a socket, so the audited path still works.

Measured on the same project: the agent completed the task through SafeScope in
ten turns, both changes recorded, nothing changed outside — and asked to create
a file with a shell redirect, it got `operation not permitted` from the kernel.

macOS only, through `sandbox-exec`; elsewhere it refuses rather than running the
command unprotected. It denies writes without exception, so a test run cannot
write `__pycache__` either. That is the cost of the guarantee.

**Drift detection** is what notices when enforcement does not hold, and without
a guard it will not always hold — a hook can be disabled, and the engine cannot check what it
never sees. A baseline is taken when a session starts; `safescope status` and
`safescope drift` then name the files that changed without the engine, which are
exactly the ones with no stored previous contents and no undo.

It detects; it does not prevent. A file it names is already changed and its
previous contents are already gone. What it buys is that the change is named
instead of being silently absent from the record — which is what "nothing was
recorded" looked like in every run above.

### What SafeScope is for

An audited path, and a policy check before a change is made:

- A change made **through** SafeScope is checked against the approved scope and
  budget, has its previous contents stored, and can be undone.
- A change made **around** it is not recorded — and `get_status` says so rather
  than implying coverage it does not have.

That is worth having when you want a reviewable record of what an agent changed
and a way back. It is not worth relying on as a boundary, and this document will
not pretend otherwise.

## Layout

```
plugin/
├── .claude-plugin/plugin.json
├── .mcp.json                 the safescope MCP server
├── hooks/hooks.json          PreToolUse, SessionStart, Stop
├── skills/                   start, plan, status, history, undo, finish
└── bin/safescope             built by scripts/build-plugin.sh
```

Skills are instructions, not enforcement. Every check they describe is also made
inside the engine, because a skill is advice a model may or may not follow.
