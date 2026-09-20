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
directly. To close most of that gap, deny the built-in edit tools in the
project's `.claude/settings.json`:

```json
{
  "permissions": {
    "deny": ["Write", "Edit", "MultiEdit", "NotebookEdit"]
  }
}
```

With those denied, a file change has to go through SafeScope's tools, and the
record is complete for every change that happens.

### The part this does not close

`Bash` remains. `sed -i`, a redirect and a script all write files, and none of
them can be read reliably enough to judge. SafeScope does not try: a check that
looked like protection without being it would be worse than none.

Two honest options:

- Leave `Bash` allowed and accept that changes made through it are unrecorded.
  `get_status` reports this rather than implying coverage it does not have.
- Deny `Bash` too, and accept that tests and builds then have to be run by hand.

There is no configuration that gives you both. Anyone who tells you otherwise is
describing a sandbox, which this is not.

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
