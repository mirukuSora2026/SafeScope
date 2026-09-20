---
name: plan
description: Prepare a file change through SafeScope and explain it before applying.
---

# Preparing a change

Two steps, always in this order.

1. `prepare_change` — checks the change against the approved scope and budget,
   stages the new contents, and stores the previous ones so the change can be
   undone. It alters nothing in the workspace.
2. `apply_change` — takes the plan id and nothing else.

The split is the point. Between the two, nothing can substitute different
contents for the ones that were checked.

## Reading a refusal

Every refusal names the rule responsible and says whether an approval could
change the answer.

- **It says an expansion is possible.** The path is outside the allowed scope, or
  the rule covers the path but not this operation. Consider
  `request_scope_expansion`, but only when the work genuinely needs that path —
  each request costs somebody's attention.
- **It says it cannot be opened by an approval.** A protected path or a deny
  rule. This will not change. Do not retry, and do not ask.

Never retry a refusal unchanged. If the code is `SOURCE_CHANGED`, rebuild the
plan against the current contents; anything else needs a different request.

## Before a large edit

Check `get_status` first. Running out of budget mid-way leaves the work half
done, which is worse than not starting.
