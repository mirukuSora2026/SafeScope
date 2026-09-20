---
name: undo
description: Reverse the last change SafeScope made, after checking it is safe to.
---

# Undo

1. `prepare_undo` — works out how to reverse the most recent completed operation.
   Changes nothing.
2. `apply_undo` — carries it out.

## When it is refused

`RECOVERY_CONFLICT` means the file has changed since SafeScope touched it.
Somebody worked on it afterwards, and reversing would throw that away.

**Do not force it. There is no way to force it, and that is on purpose.** Tell
the person:

- which file it is
- that the previous contents are still stored and nothing has been lost
- that they need to decide what should survive

Then stop and let them decide. Offering to "just overwrite it" is offering to
destroy work you cannot see.

## What undo does not need

Undo does not spend the change budget and is not checked against the policy. It
reverses what SafeScope did under a policy that was already approved, so a spent
budget or a since-narrowed rule cannot leave somebody stuck with a change they
cannot reverse.
