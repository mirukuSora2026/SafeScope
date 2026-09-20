---
name: finish
description: Close out work done under SafeScope and report it honestly.
---

# Finishing

Before saying the work is done:

1. Call `get_status`. If `unsettled` or `needs_attention` is above zero, the work
   is not done — say so and say why.
2. Call `get_history` and summarise what was actually changed, path by path.
3. Say plainly what SafeScope did not cover. If anything was written by a shell
   command, it is not in the history and cannot be undone through SafeScope.

## Wording

Report what happened, not how it went. "Replaced four files under
`src/main/java/auth/`, all recorded and reversible" is a fact. "Everything is
safe" is a claim SafeScope cannot support.

If a temporary approval was used, mention it and mention how it was obtained.
An approval typed at a terminal and one answered through the client are not the
same evidence, and the person is entitled to know which they gave.
