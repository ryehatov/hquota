---
name: quota
description: Report current Codex and Command Code quota using the local hquota broker.
---

# Current quota

For one user quota request, invoke `hquota --json` through `terminal` once at most.
Do not invoke provider endpoints, read credential files, or request credentials.

Require `schema_version` to equal `1` exactly. If the command fails, JSON is malformed,
or the version differs, report the failure. Do not invent or reuse stale quota values.

Use only normalized facts in the returned report. Identify each account by the pair
`(provider, account)`. Explain account-local errors without requesting secrets.
Show rolling windows, balances, spend limits, explicit availability and reset times
separately. Missing facts are unknown, not zero. Report the observation time.

Compare `headroom_percent` only when both values exist and their primary quota
semantics are comparable. For example, compare two ordinary Codex accounts.
Headroom is not proof that an account is usable. Provider-explicit availability is
separate evidence. Never combine auxiliary balances with rolling quota into a score
or rank unlike scopes using a fabricated common score.

Do not choose, switch or modify accounts. Do not route future work automatically.
Do not refresh credentials or perform billing actions. This Skill observes state only.
