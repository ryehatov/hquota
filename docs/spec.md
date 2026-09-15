# hquota Domain Specification

**Status:** Normative  
**Version:** 1  
**Date:** 2026-09-15  
**Implementation target:** Rust utility used by Hermes Agent

## 1. Purpose

`hquota` provides a current, read-only view of quota-related state for configured Codex and Command Code accounts.

The system has two consumers:

1. a human operator who runs the `hquota` CLI;
2. Hermes Agent, which reads versioned JSON through a `quota` Skill.

The system is an observation interface. It is not an account manager, policy engine, scheduler, credential manager, or billing ledger.

## 2. Normative language

The key words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** define requirements in this document.

## 3. Scope

The system MUST provide:

- multiple configured Codex accounts;
- multiple configured Command Code accounts;
- live, on-demand quota acquisition;
- current rolling-window state;
- current balance, spend-limit, and explicit availability state when the selected provider endpoint exposes them;
- account-local failure isolation;
- human-readable terminal output;
- versioned JSON for Hermes;
- a Unix-socket boundary between Hermes and credential-facing code;
- a live diagnostic command.

The system MUST NOT provide:

- historical samples;
- scheduled polling;
- background collection;
- persistent quota state;
- OAuth refresh;
- credential creation or modification;
- account switching;
- automatic account selection;
- automatic routing based on quota;
- credential storage;
- provider billing actions;
- credit purchase or reset-credit redemption;
- a web interface;
- a Hermes Python plugin.

## 4. System boundary

The required runtime topology is:

```text
Human / Hermes
      |
      | hquota client
      v
Unix domain socket
      |
      v
hquota broker
  |        |
  |        +-- Command Code API-key files
  +----------- Codex auth.json mounts
      |
      v
fixed provider HTTPS origins
```

Hermes MUST NOT receive provider credentials.

The broker is the only component that MAY read provider credentials.

The broker MUST be stateless across quota requests, except for immutable startup configuration and process-local runtime objects such as the listening socket and HTTP client.

## 5. Trust model

### 5.1 Assets

The protected assets are:

- Codex access tokens;
- Codex account identifiers when present in credential files;
- Command Code API keys;
- credential file contents;
- provider response bodies before normalization.

### 5.2 Trusted components

The deployment trusts:

- the human operator;
- the local host and Docker daemon;
- the `hquota` broker binary;
- the `hquota` client binary;
- the explicitly configured provider origins.

Hermes is trusted to read normalized quota state, but it is not trusted with provider credentials.

### 5.3 Adversary and failure model

The system MUST tolerate:

- one account having expired credentials;
- one provider timing out;
- one provider being unavailable;
- provider response schema drift;
- a provider returning only a subset of expected quota windows;
- additive unknown provider response fields;
- concurrent local client requests.

The PoC does not attempt to defend against a malicious process running as the same Unix UID as Hermes and the broker. Such a process can access the same `0600` socket by definition.

## 6. Provider and account identity

### 6.1 Provider

The supported providers are:

```text
codex
command-code
```

No generic provider registry is part of version 1.

### 6.2 Account identity

An account identity is the pair:

```text
(provider, account)
```

`account` is an operator-defined stable name. It MUST match:

```text
[a-z0-9][a-z0-9-]*
```

The pair `(provider, account)` MUST be unique in the configured account registry.

Provider-owned account IDs are metadata used only by provider adapters. They are not the public account identity.

## 7. Configuration contract

The broker MUST load one strict JSON configuration file at startup.

Default path:

```text
/etc/hquota/config.json
```

The server command MAY override the path with:

```text
hquota serve --config PATH
```

The configuration MUST NOT contain secret values.

The version 1 schema is provider-tagged:

```json
{
  "schema_version": 1,
  "accounts": [
    {
      "provider": "codex",
      "name": "business",
      "auth_json": "/credentials/codex/business/auth.json"
    },
    {
      "provider": "command-code",
      "name": "goat",
      "api_key_file": "/run/secrets/command-code-goat"
    }
  ]
}
```

Rules:

- `schema_version` MUST equal `1` exactly.
- Unknown configuration fields MUST cause startup failure.
- A Codex account MUST contain `auth_json` and MUST NOT contain `api_key_file`.
- A Command Code account MUST contain `api_key_file` and MUST NOT contain `auth_json`.
- Credential paths MUST be absolute paths.
- Duplicate `(provider, name)` pairs MUST cause startup failure.
- An empty `accounts` array is valid.
- Account topology changes require a broker restart.
- Credential file contents MUST be read again for each quota or diagnostic request. Credential rotation therefore does not require a broker restart.

## 8. Credential contract

### 8.1 General rules

All broker credential inputs MUST be filesystem paths.

The broker MUST NOT accept provider secrets through:

- CLI arguments;
- broker configuration values;
- broker environment variables;
- client requests;
- Hermes prompts.

The broker MUST NOT persist credentials.

The broker MUST NOT cache credentials between requests.

The broker MUST NOT modify credential files.

### 8.2 Codex credentials

For version 1, the configured `auth.json` MUST use the current Codex ChatGPT-token shape and contain:

```text
tokens.access_token
tokens.account_id
```

Both values MUST be non-empty strings.

The broker MUST NOT:

- decode an ID token to recover a missing account ID;
- use a refresh token;
- refresh an access token;
- support Codex API-key authentication as a substitute for ChatGPT subscription quota authentication.

A missing or incompatible field is a credential error.

### 8.3 Command Code credentials

The configured API-key file MUST contain exactly one API key after surrounding ASCII whitespace is removed. After trimming, any remaining ASCII whitespace is invalid.

An empty value is invalid.

The key MUST remain a broker-local value.

## 9. Domain model

The public domain contains:

```text
QuotaReport
AccountQuota
QuotaScope
QuotaFact
AccountError
```

### 9.1 QuotaReport

A report is one broker acquisition operation.

Conceptual type:

```rust
struct QuotaReport {
    schema_version: u32,
    fetched_at: Timestamp,
    accounts: Vec<AccountQuota>,
}
```

`schema_version` MUST equal `1`.

`fetched_at` MUST be an RFC 3339 UTC timestamp. It represents report completion time.

### 9.2 AccountQuota

Conceptual type:

```rust
struct AccountQuota {
    provider: Provider,
    account: String,
    status: AccountStatus,
    primary_scope: Option<String>,
    headroom_percent: Option<Percent>,
    scopes: Vec<QuotaScope>,
    facts: Vec<QuotaFact>,
    error: Option<AccountError>,
}
```

`status` is one of:

```text
ok
error
```

If `status == "ok"`:

- `error` MUST be `null`;
- `scopes` and `facts` MAY be empty when the provider returned a valid response with no representable quota facts;
- the snapshot MUST contain only facts derived from one successful provider response operation for that account.

If `status == "error"`:

- `error` MUST be present;
- `primary_scope` MUST be `null`;
- `headroom_percent` MUST be `null`;
- `scopes` MUST be empty;
- `facts` MUST be empty.

There is no partial account state in schema version 1.

### 9.3 QuotaScope

Conceptual type:

```rust
struct QuotaScope {
    id: String,
    role: ScopeRole,
    label: Option<String>,
}
```

`role` is one of:

```text
primary
auxiliary
unknown
```

A successful account MUST contain at most one scope with role `primary`.

If one primary scope exists, `primary_scope` MUST equal its `id`.

If no primary scope exists, `primary_scope` MUST be `null`.

Scope IDs are stable normalized identifiers chosen by the adapter. They are not display strings.

### 9.4 Percent

A percentage is an integer in the inclusive range:

```text
0..100
```

When an upstream provider supplies a fractional used percentage, normalization MUST conservatively round the used percentage upward and then clamp it to `0..100`.

For a normalized rate window:

```text
remaining_percent = 100 - used_percent
```

The invariant is:

```text
used_percent + remaining_percent = 100
```

### 9.5 Quantity

Continuous provider quantities MAY use IEEE-754 `f64` in the public schema. Every public floating-point value MUST be finite. `NaN`, positive infinity, and negative infinity are invalid upstream data and MUST cause account normalization to fail.

Examples include:

- credit balances;
- credit consumption;
- spend values;
- provider-native continuous usage units.

Discrete values MUST use integer representations. Examples include:

- percentages;
- durations;
- timestamps before formatting;
- counts.

Quantity units in version 1 are:

```text
credits
provider_units
```

An adapter MUST NOT label a provider value as a currency unless the provider contract unambiguously defines it as that currency. Version 1 therefore does not define a currency unit.

### 9.6 QuotaFact

`QuotaFact` is a tagged union. Each fact MUST reference one declared `scope` ID.

The supported fact types are:

```text
rate_window
balance
spend_limit
availability
```

#### 9.6.1 Rate window

```json
{
  "type": "rate_window",
  "scope": "codex",
  "duration_seconds": 18000,
  "canonical_window": "5h",
  "used_percent": 22,
  "remaining_percent": 78,
  "reset_at": "2026-09-15T11:43:00Z",
  "used_value": null,
  "limit_value": null,
  "unit": null
}
```

Fields:

- `duration_seconds`: positive integer or `null` if the provider did not expose duration;
- `canonical_window`: `"5h"`, `"7d"`, or `null`;
- `used_percent`: required integer `0..100`;
- `remaining_percent`: required integer `0..100`;
- `reset_at`: RFC 3339 UTC timestamp or `null`;
- `used_value`, `limit_value`, `unit`: optional quantity triplet.

The quantity triplet MUST be either fully present or fully `null`.

Canonical classification MUST use exact duration matching:

```text
18000 seconds  -> 5h
604800 seconds -> 7d
all other durations -> null
```

The adapter MUST NOT infer a canonical window from field position, ordering, or a broad duration heuristic.

#### 9.6.2 Balance

```json
{
  "type": "balance",
  "scope": "extra-credits",
  "value": 12.5,
  "unit": "credits",
  "unlimited": false
}
```

`value` MAY be `null` when the provider explicitly reports a balance facility but does not expose a numeric amount.

If `unlimited == true`, `value` MAY be `null`.

A balance fact MUST describe a provider-reported current balance. It MUST NOT be reconstructed from historical local usage.

#### 9.6.3 Spend limit

```json
{
  "type": "spend_limit",
  "scope": "individual-spend-control",
  "used_value": 20.0,
  "limit_value": 50.0,
  "unit": "provider_units",
  "remaining_percent": 60,
  "reset_at": "2026-10-01T00:00:00Z"
}
```

`remaining_percent` is required when the provider exposes a reliable percentage. The adapter MUST NOT invent a percentage when units or semantics are ambiguous.

#### 9.6.4 Availability

```json
{
  "type": "availability",
  "scope": "codex",
  "allowed": false,
  "reason_code": "rate_limit_reached"
}
```

`reason_code` MAY be `null`.

`reason_code` MUST be a normalized machine code. It MUST NOT contain provider prose, response bodies, credentials, or arbitrary error messages.

Unknown provider reasons MAY normalize to:

```text
provider_restriction
```

## 10. Primary scope and headroom

### 10.1 Primary-scope selection

The adapter, not Hermes, determines scope roles.

Codex rules:

- the top-level ordinary Codex rate-limit pool is scope `codex` with role `primary`;
- additional metered or reserve pools are `auxiliary` unless a future specification explicitly changes this rule;
- absence of the ordinary Codex pool MUST NOT cause an auxiliary pool to be promoted to primary.

Command Code rules:

- rolling limits for included subscription usage are scope `included` with role `primary`;
- purchased, free, or other extra-credit balances are auxiliary;
- other provider controls are auxiliary unless their semantics explicitly represent ordinary included rolling quota.

### 10.2 Headroom

`headroom_percent` is a derived comparison aid. It is not an availability decision.

It is defined only when:

1. exactly one primary scope exists; and
2. that primary scope contains at least one `rate_window` fact.

Then:

```text
headroom_percent = min(remaining_percent of all rate_window facts in the primary scope)
```

Otherwise:

```text
headroom_percent = null
```

Balances, spend limits, auxiliary scopes, and availability facts MUST NOT be folded into `headroom_percent`.

A consumer MUST NOT infer that an account is usable from `headroom_percent` alone.

## 11. Provider acquisition semantics

### 11.1 General rules

Each quota request MUST perform a fresh provider request for every selected configured account.

There is no quota-response cache in version 1.

Accounts MUST be independent. One account failure MUST NOT discard successful results from other accounts.

The system MUST NOT perform automatic retries.

Target timeouts are:

```text
provider connect timeout: 3 seconds
provider request timeout: 10 seconds
client/broker I/O timeout: 15 seconds
```

### 11.2 Codex

Version 1 reads the current Codex `auth.json` token fields and calls the fixed first-party ChatGPT quota origin used by current Codex implementations:

```text
GET https://chatgpt.com/backend-api/wham/usage
```

The adapter MUST treat this as an unstable provider wire interface, not as the hquota domain contract.

The adapter MUST tolerate missing optional windows.

The adapter MUST normalize all supported current quota facts without requiring both 5-hour and 7-day windows to exist.

### 11.3 Command Code

Version 1 calls only:

```text
GET https://api.commandcode.ai/alpha/billing/credits
```

The adapter MAY normalize facts exposed by that response, including:

- included rolling 5-hour state;
- included rolling weekly state;
- monthly credit state;
- purchased credit state;
- free credit state.

Version 1 MUST NOT call additional Command Code billing or subscription endpoints to reconstruct the provider UI.

## 12. Schema-drift policy

Provider wire schemas are not the public hquota schema.

Normal quota acquisition MUST:

- ignore additive unknown provider fields;
- accept absence of documented optional provider fields;
- fail the account atomically when a present known semantic field has an incompatible type;
- fail the account atomically when a required semantic field for an observed object is missing;
- fail the account atomically when normalized data violates domain invariants.

`hquota doctor` MUST additionally report additive unknown fields in known provider objects as warnings when the adapter can detect them.

An additive unknown field alone MUST NOT make `doctor` fail.

## 13. Error contract

Account-local public error codes are a closed version 1 set:

```text
credential_missing
credential_invalid
authentication_required
timeout
upstream_unavailable
upstream_schema_changed
```

Example:

```json
{
  "provider": "codex",
  "account": "personal",
  "status": "error",
  "primary_scope": null,
  "headroom_percent": null,
  "scopes": [],
  "facts": [],
  "error": {
    "code": "authentication_required"
  }
}
```

The public error object MUST NOT include:

- access tokens;
- API keys;
- Authorization headers;
- JWTs;
- raw provider response bodies;
- provider arbitrary error prose;
- credential file contents.

## 14. Public JSON contract

`hquota --json` MUST emit exactly one `QuotaReport` JSON value to standard output.

It MUST NOT emit presentation sections, terminal bars, ANSI state, provider raw JSON, or logs in that JSON.

Example:

```json
{
  "schema_version": 1,
  "fetched_at": "2026-09-15T09:00:00Z",
  "accounts": [
    {
      "provider": "codex",
      "account": "business",
      "status": "ok",
      "primary_scope": "codex",
      "headroom_percent": 52,
      "scopes": [
        {
          "id": "codex",
          "role": "primary",
          "label": null
        }
      ],
      "facts": [
        {
          "type": "rate_window",
          "scope": "codex",
          "duration_seconds": 18000,
          "canonical_window": "5h",
          "used_percent": 22,
          "remaining_percent": 78,
          "reset_at": "2026-09-15T11:43:00Z",
          "used_value": null,
          "limit_value": null,
          "unit": null
        },
        {
          "type": "rate_window",
          "scope": "codex",
          "duration_seconds": 604800,
          "canonical_window": "7d",
          "used_percent": 48,
          "remaining_percent": 52,
          "reset_at": "2026-09-19T18:00:00Z",
          "used_value": null,
          "limit_value": null,
          "unit": null
        }
      ],
      "error": null
    }
  ]
}
```

Consumers MUST use `schema_version` as an exact compatibility check.

A breaking JSON change MUST increment `schema_version`.

The implementation is not required to continue emitting older schema versions.

## 15. CLI contract

Version 1 provides:

```text
hquota [--json] [--provider PROVIDER] [--account ACCOUNT]
hquota doctor
hquota serve [--config PATH]
```

The default invocation is the current quota command.

`--provider` and `--account` are client-side selectors. They MUST NOT cause extra provider fetches for one user invocation.

The client SHOULD request one complete report from the broker and then apply selectors locally.

Unknown arguments, missing option values, duplicate scalar options, and invalid combinations MUST cause a non-zero CLI exit.

There is no `--window` selector in version 1.

### 15.1 Quota exit status

If the broker successfully produces a `QuotaReport`, the quota CLI MUST exit `0` even when one or more accounts have `status == "error"`.

The quota CLI MUST exit non-zero for failures such as:

- no broker connection;
- protocol mismatch;
- malformed broker response;
- invalid local CLI arguments.

### 15.2 Doctor exit status

`hquota doctor` MUST perform live checks.

It MUST exit non-zero when any configured account has a credential, authentication, timeout, provider, or schema failure.

Warnings for additive unknown fields alone MUST NOT cause a non-zero exit.

### 15.3 Human output

Human terminal output is not a stable machine interface.

Consumers MUST NOT parse it.

The renderer SHOULD:

- group by provider and account;
- show the primary scope before auxiliary scopes;
- render `5h` and `7d` for canonical windows;
- render the raw duration for non-canonical windows when useful;
- show balance, spend-limit, and availability facts as separate facts;
- show account-local errors in the account position;
- use a compact 10-cell bar for percentages;
- avoid a chart or terminal UI framework.

ANSI color is not required.

## 16. Broker protocol contract

The client and broker communicate through one Unix domain socket.

Default socket path:

```text
/run/hquota/hquota.sock
```

Each connection contains exactly:

```text
one request JSON object
one response JSON object
EOF
```

There is no streaming and no multiplexing.

The client MUST:

1. connect;
2. write one JSON request;
3. shut down its write side;
4. read one JSON response until EOF;
5. close.

Supported operations are:

```text
quota
health
doctor
```

Each request MUST contain:

```json
{
  "protocol_version": 1,
  "op": "quota"
}
```

`protocol_version` is a fail-fast compatibility field.

Only exact version equality is supported.

A breaking protocol change MUST increment the version. The implementation is not required to support old protocol versions.

## 17. Socket and process security

The shared runtime directory MUST have mode:

```text
0700
```

The socket MUST have mode:

```text
0600
```

The broker and Hermes-side client MUST run under the same intended non-root UID.

The broker MUST run as non-root.

The broker MUST NOT receive Hermes runtime data volumes that it does not require.

Hermes MUST NOT receive provider credential mounts.

Codex credential mounts MUST be read-only.

Command Code secret files MUST be read-only.

## 18. Outbound network security

Each provider adapter MUST use a fixed HTTPS origin compiled into the binary.

Version 1 origins are:

```text
https://chatgpt.com
https://api.commandcode.ai
```

The broker MUST NOT accept a provider base URL from configuration, environment variables, client input, or Hermes.

Credential-bearing HTTP requests MUST NOT follow redirects.

System and environment proxy auto-discovery MUST be disabled.

The broker MUST NOT send provider credentials to any origin other than the fixed origin for that provider.

## 19. Logging contract

Operational logs MAY go to standard error.

Permitted examples:

```text
hquota: listening on /run/hquota/hquota.sock
hquota: codex/business fetch failed: authentication_required
hquota: command-code/goat fetch failed: timeout
```

Logs MUST NOT contain:

- credential values;
- complete credential files;
- Authorization headers;
- JWTs;
- raw provider response bodies;
- arbitrary provider error bodies.

`hquota doctor` is the supported diagnostic surface.

## 20. Hermes Skill contract

Hermes integration MUST use one Skill named `quota` that invokes the external `hquota` CLI.

For one user quota request, the Skill SHOULD invoke:

```text
hquota --json
```

at most once.

The Skill MAY:

- report current normalized facts;
- state reset times;
- compare `headroom_percent` between accounts when both values exist and have the same primary-quota semantics;
- report provider-explicit availability facts;
- explain account-local errors.

The Skill MUST NOT:

- request or display credentials;
- switch accounts;
- modify provider credentials;
- choose an account on the user's behalf;
- route future work automatically to an account;
- combine auxiliary balances with rolling windows into one score;
- infer availability from headroom alone;
- rank incomparable provider scopes using a fabricated common score.

Example permitted statement:

```text
Personal has 11 percentage points more primary quota headroom than Business.
```

Example prohibited policy conclusion:

```text
Use Personal for the next task.
```

unless the user separately asks Hermes to make that decision under another explicit policy.

## 21. Verification requirements

Continuous integration MUST be usable without real provider credentials.

CI MUST cover at least:

- strict configuration parsing;
- duplicate account rejection;
- credential-shape parsing with synthetic credentials;
- provider wire fixtures with secrets removed;
- missing-window cases;
- unknown additive provider fields;
- known-field type changes;
- quota normalization invariants;
- canonical-window exact matching;
- primary-scope selection;
- headroom derivation;
- atomic account failure;
- multi-account aggregation;
- protocol exact-version behavior;
- public JSON snapshots;
- human-renderer smoke tests;
- HTTP redirect rejection configuration;
- proxy-disabled configuration;
- secret-redaction behavior.

Live provider tests MUST be opt-in and operator-controlled.

Live tests MUST use only `hquota` to access credentials.

Raw live provider responses MUST NOT be committed as test artifacts.

## 22. Acceptance criteria

The PoC is accepted when one Docker Compose deployment contains:

```text
Hermes Agent
hquota broker
2 Codex accounts
1 Command Code account
```

and satisfies all of the following:

1. `hquota` returns all three account results in one invocation.
2. `hquota --json` emits schema version 1 and contains only normalized domain data.
3. `hquota --provider codex` filters the rendered result without a second provider acquisition for that invocation.
4. `hquota --account business` selects the configured account.
5. `hquota doctor` performs credential, live provider, schema, and normalization checks.
6. One failed account does not suppress successful accounts in a normal quota report.
7. Hermes can answer a current quota overview through the `quota` Skill.
8. Hermes can compare primary quota headroom for comparable accounts.
9. Hermes has no mounted Codex credential files.
10. Hermes has no mounted Command Code API-key secret.
11. The broker is non-root.
12. Broker credential mounts are read-only.
13. The runtime directory is `0700` and socket is `0600`.
14. No credential appears in CLI output, broker logs, public JSON, protocol errors, or diagnostic output.
15. Credential-bearing requests cannot be redirected to another HTTP origin.
16. No database, history service, scheduler, cache, account switcher, or OAuth refresh subsystem exists.

## 23. Deliberate non-goals

The following features require a new specification decision before implementation:

- persistence;
- time-series history;
- scheduled collection;
- stale-result fallback;
- quota caching;
- automatic retry;
- account rotation or load balancing;
- policy-based account selection;
- credential refresh;
- interactive login;
- provider write operations;
- user-configurable provider origins;
- proxy support;
- additional Command Code endpoints;
- generic third-party provider plugins;
- backward-compatible schema multiplexing.
