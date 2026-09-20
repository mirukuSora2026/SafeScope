---
name: status
description: Report what SafeScope has changed, what is left, and what it does not cover.
---

# Status

`get_status` returns:

- **allowed** — the patterns in force and which operations each permits
- **changed_paths / operations / moves** — what this task has spent
- **recovery_storage_bytes** — how much undo data is held
- **unsettled / needs_attention** — operations that did not finish, and of those
  the ones a person has to compare
- **unapproved_policy_edits** — the policy file was changed but not approved, so
  whoever changed it probably believes it is in force and it is not
- **temporary_approvals** — how many expansions are open for this task
- **coverage** — what SafeScope does not see

## Reporting it to a person

Give them the numbers, not a verdict. "Three of eight paths, seven of twenty
operations" is checkable against what they can see; "well within limits" is not.

If `needs_attention` is above zero, say so first and stop. The engine could not
determine what happened to those operations, and nothing else is worth
discussing until somebody has looked.

Never describe the project as protected. Say what was recorded and what was not.
