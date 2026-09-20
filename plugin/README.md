# SafeScope plugin

Keeps file changes inside an approved scope and budget, and records them so they
can be reviewed and undone.

## What it does not do

**SafeScope is not a sandbox.** It guarantees that changes made *through its own
tools* carry a complete chain of scope, budget and recovery data. A file written
by a shell command is not recorded and cannot be undone through it.

The status output says so, and so should you.

## Installing

```bash
./scripts/build-plugin.sh     # builds the engine into plugin/bin/
```

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

**Drift detection** is what notices when enforcement does not hold, and it will
not always hold — a hook can be disabled, and the engine cannot check what it
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
