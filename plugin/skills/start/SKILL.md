---
name: start
description: Set up SafeScope in this project, or check what it is already allowed to change.
---

# Starting work under SafeScope

SafeScope permits nothing until a person approves a policy. That is deliberate:
a default that allowed something would mean nobody reads the policy.

## If the project has never used SafeScope

1. Run `safescope init`. It writes `.safescope/policy.toml` with an empty allow
   list and the standard refusals.
2. Ask the person which paths this work needs, and fill in `allow`. Use the
   narrowest patterns that cover the work — `src/main/java/auth/**`, not `src/**`.
3. Tell them to run `safescope policy approve` themselves. It has to be typed at
   a terminal; you cannot do this step for them, and you should not try.

## If it is already set up

Call `get_status`. It reports what is allowed, what this task has spent, and
anything left unsettled. Read the `coverage` field aloud if the person seems to
expect more than SafeScope provides.

## What this does not do

SafeScope records changes made through its own tools. A file written by a shell
command is not recorded and cannot be undone through it. Do not describe the
project as protected; describe it as audited where SafeScope was used.
