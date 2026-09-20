---
name: history
description: Show what SafeScope actually changed, and what can still be undone.
---

# History

`get_history` returns this task's operations, most recent first, with what became
of each.

- **kind** — `change` or `undo`. An undo is recorded like any other operation but
  does not spend the change budget.
- **stage** — `committed` means it happened and the outcome was observed.
  `conflict` or `recovery_required` mean the engine cannot say, and a person has
  to compare.
- **error_code** — present when an operation could not be settled cleanly.

## What is undoable

Only the most recent completed change, and only if the file is still as SafeScope
left it. Undo walks backwards one operation at a time; there is no way to reverse
something from the middle.

Report history as a record of what happened, not as a summary of the current
state. A file changed three times and then edited by hand is three entries and a
file SafeScope no longer recognises.
