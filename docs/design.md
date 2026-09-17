# hquota Implementation Design

**Status:** Normative implementation specification  
**Version:** 1  
**Date:** 2026-09-17  
**Language:** Rust

## 1. Design objective

Implement the domain contract in `spec.md` with the smallest practical code base.

The implementation must keep provider credentials outside Hermes, perform fresh on-demand reads, and expose only normalized current-state data.

This design intentionally avoids framework and compatibility infrastructure that is not required by version 1.

## 2. Architecture

One Rust crate builds one executable:

```text
hquota
```

The executable has two runtime roles:

```text
hquota serve     credential-facing broker
hquota ...       unprivileged local client
```

Deployment:

```text
+-----------------------------+
| Hermes container            |
|                             |
| quota Skill                 |
| hquota client               |
+--------------+--------------+
               |
               | /run/hquota/hquota.sock
               v
+-----------------------------+
| hquota broker container     |
|                             |
| strict config               |
| Codex auth mounts           |
| Command Code secret files   |
+--------------+--------------+
               |
       fixed HTTPS origins
          /             \
         v               v
     ChatGPT       Command Code
```

No credential-bearing provider code runs in the Hermes process.

## 3. Crate structure

Use one crate and no Cargo workspace.

Recommended source tree:

```text
hquota/
├── Cargo.toml
├── Cargo.lock
├── Dockerfile
└── src/
    ├── main.rs
    ├── cli.rs
    ├── config.rs
    ├── model.rs
    ├── protocol.rs
    ├── broker.rs
    ├── client.rs
    ├── http.rs
    ├── codex.rs
    ├── command_code.rs
    ├── normalize.rs
    ├── render.rs
    └── doctor.rs
```

This is a file organization, not an abstraction requirement. If implementation stays clearer with fewer files, merge modules.

Do not create:

- a provider trait;
- a provider registry;
- a dependency-injection framework;
- a repository layer;
- a service layer;
- a plugin interface;
- a persistence abstraction.

There are exactly two provider adapters in version 1. Direct functions are sufficient.

## 4. Dependency set

Production dependencies SHOULD be limited to:

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = {
    version = "0.13",
    default-features = false,
    features = ["blocking", "json", "rustls"]
}
time = { version = "0.3", features = ["formatting", "parsing", "serde"] }
```

Use the Rust standard library for:

- argument parsing;
- threads;
- Unix sockets;
- filesystem access;
- paths;
- process exit codes;
- stderr logging;
- timeouts on Unix sockets.

Do not add in version 1:

```text
clap
Tokio
async-std
chrono
base64
tracing
log
anyhow
thiserror
web frameworks
terminal UI frameworks
```

A new dependency requires a concrete version 1 requirement that cannot be implemented clearly with the existing set.

## 5. CLI parser

Parse `std::env::args_os()` directly.

Accepted forms:

```text
hquota [--json] [--provider PROVIDER] [--account ACCOUNT]
hquota doctor
hquota serve [--config PATH]
```

Implementation rules:

- parse `OsString` until a UTF-8 value is required by the public contract;
- reject an unknown flag;
- reject duplicate `--provider`;
- reject duplicate `--account`;
- reject duplicate `--config`;
- reject missing option values;
- reject quota-only flags under `serve` or `doctor`;
- reject positional arguments not defined above;
- print usage to stderr on syntax failure;
- exit non-zero on syntax failure.

Do not implement aliases or deprecated forms.

## 6. Configuration implementation

### 6.1 Serde types

Use a tagged enum and strict unknown-field rejection.

Conceptual Rust:

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u32,
    accounts: Vec<AccountConfig>,
}

#[derive(Deserialize)]
#[serde(tag = "provider", rename_all = "kebab-case")]
enum AccountConfig {
    Codex {
        name: String,
        auth_json: PathBuf,
    },
    CommandCode {
        name: String,
        api_key_file: PathBuf,
    },
}
```

Because Serde enum variants do not automatically deny variant-specific unknown fields in every layout, add explicit variant record structs with `#[serde(deny_unknown_fields)]` if needed to guarantee the `spec.md` rule.

### 6.2 Validation

After parsing:

1. require `schema_version == 1`;
2. validate every account name against `[a-z0-9][a-z0-9-]*` with a small ASCII byte check; do not add a regex dependency;
3. require credential paths to be absolute;
4. reject duplicate `(provider, name)` pairs;
5. keep account ordering from the configuration file for deterministic output.

Do not canonicalize credential paths at startup. Atomic host-side replacement of a mounted credential file must remain visible to later requests.

Do not read credential file contents during normal broker startup. Credential validity belongs to quota acquisition and `doctor`.

## 7. Secret-bearing internal types

Secret-bearing types must have the smallest lifetime that is practical.

Example:

```rust
struct CodexCredential {
    access_token: String,
    account_id: String,
}

struct CommandCodeCredential(String);
```

Do not derive `Debug` for these types.

Do not store them in broker-global state.

Construct them inside the account fetch operation and drop them when that operation returns.

Rust `String` does not guarantee zeroization. Version 1 does not add a zeroization crate. The security boundary is instead based on short lifetime, no logging, no persistence, and no cross-process exposure.

## 8. Public model types

`model.rs` implements the normative schema from `spec.md`.

Recommended shape:

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Provider {
    Codex,
    CommandCode,
}

#[derive(Serialize)]
struct QuotaReport {
    schema_version: u32,
    fetched_at: String,
    accounts: Vec<AccountQuota>,
}

#[derive(Serialize)]
struct AccountQuota {
    provider: Provider,
    account: String,
    status: AccountStatus,
    primary_scope: Option<String>,
    headroom_percent: Option<u8>,
    scopes: Vec<QuotaScope>,
    facts: Vec<QuotaFact>,
    error: Option<AccountError>,
}
```

Use a tagged enum for facts:

```rust
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum QuotaFact {
    RateWindow { /* fields from spec */ },
    Balance { /* fields from spec */ },
    SpendLimit { /* fields from spec */ },
    Availability { /* fields from spec */ },
}
```

Do not expose provider wire DTOs from `model.rs`.

## 9. Percentage normalization

Use one helper for percentage normalization.

Conceptual logic:

```rust
fn normalize_used_percent(raw: f64) -> Result<u8, NormalizeError> {
    if !raw.is_finite() {
        return Err(NormalizeError::InvalidPercent);
    }

    let used = raw.ceil().clamp(0.0, 100.0) as u8;
    Ok(used)
}
```

Then:

```text
remaining = 100 - used
```

For an upstream integer percentage, convert to `f64` only if a shared helper requires it. A direct integer validation path is preferable.

Reject values that are structurally invalid before normalization when the provider contract defines a narrower valid range.

## 10. Time representation

Provider DTOs may use Unix seconds.

Convert public times to RFC 3339 UTC at the normalization boundary.

Use `time::OffsetDateTime::from_unix_timestamp` and a fixed RFC 3339 formatter.

Do not use local time in public JSON.

The human renderer may show local-friendly relative reset durations. It must derive those from the normalized UTC timestamp.

## 11. Scope normalization

### 11.1 Codex

Use:

```text
scope id: codex
role: primary
```

for the ordinary top-level Codex quota pool.

For additional rate-limit pools:

1. use the provider metered-feature identifier when available;
2. otherwise use the provider limit identifier/name after deterministic normalization;
3. set role to `auxiliary`;
4. retain a human label when the provider supplies one.

Do not promote an additional scope when the ordinary scope is absent.

If a provider identifier cannot be converted into a safe stable scope ID without guessing, use a deterministic adapter-generated ID and preserve the provider label separately.

### 11.2 Command Code

Use:

```text
scope id: included
role: primary
```

for included-subscription rolling windows.

Use separate auxiliary scopes for balances that have different semantics, for example:

```text
monthly-credits
purchased-credits
free-credits
```

Do not combine them into the `included` rolling scope.

## 12. Headroom derivation

Implement one pure function over normalized facts.

Pseudo-code:

```text
primary = scopes where role == primary
if count(primary) != 1:
    return null

windows = rate_window facts where fact.scope == primary.id
if windows is empty:
    return null

return min(window.remaining_percent)
```

Do not inspect balances, availability, spend limits, provider names, or canonical window labels in this calculation.

Unit-test this function independently.

## 13. HTTP client hardening

Create one blocking `reqwest::blocking::Client` during broker startup and share clones across request threads.

Build it explicitly:

```rust
Client::builder()
    .connect_timeout(Duration::from_secs(3))
    .timeout(Duration::from_secs(10))
    .redirect(reqwest::redirect::Policy::none())
    .no_proxy()
    .build()
```

Also:

- compile with `default-features = false`;
- enable `rustls` explicitly;
- do not enable `system-proxy`;
- do not enable cookies;
- do not enable HTTP/2 unless a provider later requires it;
- do not accept a provider URL from config or environment;
- construct request URLs from string constants only.

Set a static, non-secret user agent such as:

```text
hquota/<crate-version>
```

Set `Accept: application/json` where appropriate.

Never log a `reqwest::Request` with headers attached.

## 14. Codex adapter

### 14.1 Credential read

For each Codex fetch:

1. open the configured `auth_json` path read-only;
2. parse only the fields needed for version 1;
3. require non-empty `tokens.access_token`;
4. require non-empty `tokens.account_id`;
5. ignore refresh-token and unrelated auth fields;
6. return `credential_missing` for file-not-found;
7. return `credential_invalid` for malformed JSON or missing required fields.

Use a minimal private DTO instead of mirroring the complete Codex auth schema.
Do not use `deny_unknown_fields` for the provider-owned credential format.

### 14.2 Request

Fixed request:

```text
GET https://chatgpt.com/backend-api/wham/usage
Authorization: Bearer <access_token>
ChatGPT-Account-Id: <account_id>
Accept: application/json
User-Agent: hquota/<version>
```

Do not follow redirects. Do not refresh on `401` or `403`. Map authentication failure to `authentication_required`. Do not include the response body in public errors or stderr logs.

### 14.3 Wire DTO strategy

Keep WHAM DTOs private to `codex.rs`. Ignore additive unknown fields, use strict Rust types for known semantic fields, and reject incompatible changes to known fields. A response is valid when only one rate window is present.

### 14.4 Window conversion

For each provider rate window:

```text
used_percent       -> normalize Percent
limit duration     -> duration_seconds
reset epoch        -> RFC 3339 UTC
18000 seconds      -> canonical_window = 5h
604800 seconds     -> canonical_window = 7d
otherwise          -> canonical_window = null
```

Do not classify by `primary_window` versus `secondary_window` position.

### 14.5 Additional limits

Preserve provider-owned identifiers through deterministic scope normalization. Every additional limit is auxiliary in version 1. A malformed known semantic field fails the account atomically with `upstream_schema_changed`.

## 15. Command Code adapter

### 15.1 Credential read

For each Command Code fetch:

1. open the configured `api_key_file` read-only;
2. read it as UTF-8 text;
3. trim surrounding ASCII whitespace;
4. reject an empty result or any remaining ASCII whitespace;
5. keep the key only for the request lifetime.

File-not-found maps to `credential_missing`. Invalid UTF-8 or an empty value maps to `credential_invalid`.

### 15.2 Request

Fixed request:

```text
GET https://api.commandcode.ai/alpha/billing/credits
Authorization: Bearer <api-key>
Accept: application/json
User-Agent: hquota/<version>
```

Do not call other Command Code endpoints in version 1.

### 15.3 Normalization

Keep Command Code wire DTOs private to `command_code.rs`. Normalize only fields whose semantics are established. Rolling windows use the returned cap; do not hard-code plan limits. Omit balance facts whose meaning is ambiguous.

## 16. Provider response atomicity

Provider parsing and normalization for one account must complete before an `AccountQuota` success value is constructed.

```text
read credential
  -> HTTP request
  -> parse complete provider DTO
  -> validate known semantics
  -> normalize scopes and facts
  -> validate domain invariants
  -> derive headroom
  -> construct AccountQuota::ok
```

Any failure before the final step returns an account error with no scopes and no facts. Do not return partial facts.

## 17. Broker concurrency

Use blocking threads only. Spawn one thread per accepted connection and one scoped thread per configured account for quota acquisition. Restore deterministic configuration order before serialization. Do not share credentials between account threads.

## 18. Unix socket lifecycle

Default path:

```text
/run/hquota/hquota.sock
```

Before binding, require the parent directory to exist, be owned by the broker UID, and have exact mode `0700`. Reject UID 0. Probe an existing socket before removing a stale socket. Set the bound socket mode to `0600`. Do not recursively delete runtime-directory contents.

Set read and write timeouts to 15 seconds. Each connection carries exactly one request and one response, framed by EOF.

## 19. Broker protocol types

Use a small internal envelope with exact `protocol_version == 1`. Operations are `quota`, `health`, and `doctor`. `health` uses process-local state only and must not read credentials or contact providers.

## 20. Doctor implementation

`doctor` performs live diagnostics. It checks credential readability and shape, provider request success, known semantic fields, normalization invariants, and detectable additive unknown fields. Never emit arbitrary provider messages or secrets.

## 21. Unknown-field diagnostics

Normal deserialization ignores unknown fields. In `doctor` mode only, parse into `serde_json::Value`, inspect known object keys against small static sets, then deserialize and normalize. Do not add a generic JSON-schema validator.

## 22. Human renderer

The renderer consumes only `QuotaReport`. It must not access provider DTOs or credentials. Keep deterministic provider/account/scope ordering. A presentation bar must not alter domain values. Do not add a TUI dependency.

## 23. Client-side filters

The client always requests one complete `QuotaReport`, then applies `--provider` and `--account` locally. An unmatched selector returns an empty filtered report and exit `0`.

## 24. Logging

Use direct `eprintln!` calls for a small set of operational events. Emit only normalized public error codes. Never print authorization headers, raw response bodies, credential values, or raw request errors that could contain sensitive URLs.

## 25. Error mapping

Use one provider-independent account error enum matching `spec.md`.

```text
credential file ENOENT           -> credential_missing
credential parse/shape invalid   -> credential_invalid
HTTP 401/403                     -> authentication_required
reqwest timeout                  -> timeout
DNS/TLS/connect/5xx/other HTTP   -> upstream_unavailable
known provider field incompatible -> upstream_schema_changed
normalization invariant failure  -> upstream_schema_changed
```

Do not automatically retry any category.

## 26. Docker images

Use one multi-stage Dockerfile with a shared Rust build and two runtime targets.

The `broker` target must remain a minimal non-root runtime with CA certificates, no compiler toolchain, and only the required executable/runtime files.

The `hermes` target is a derivative client image:

```dockerfile
ARG HERMES_BASE_IMAGE=nousresearch/hermes-agent:latest
FROM ${HERMES_BASE_IMAGE} AS hermes
COPY --from=build /src/target/release/hquota /usr/local/bin/hquota
```

Do not override `USER`, `ENTRYPOINT`, or `CMD` in the Hermes target. Current Hermes images own their s6 bootstrap and runtime privilege drop. The derivative image adds the client binary only. The quota Skill is installed by the external Hermes stack into the persistent Hermes Skill directory rather than copied into an unused image path.

## 27. Docker Compose integration

The repository Compose file owns only the broker. An external Hermes stack owns Hermes and any search or other adjacent services.

Use a host directory `./run/hquota` for the shared socket. Before startup it must be owned by the intended non-root UID and have exact mode `0700`.

The broker Compose service should use:

```yaml
services:
  hquota:
    image: hquota-broker:local
    container_name: hermes-hquota
    user: "${HERMES_UID}:${HERMES_GID}"
    command: ["serve", "--config", "/etc/hquota/config.json"]
    read_only: true
    cap_drop: [ALL]
    security_opt: [no-new-privileges:true]
    volumes:
      - ./run/hquota:/run/hquota
      - ./config.json:/etc/hquota/config.json:ro
      - ${CODEX_BUSINESS_HOME}:/credentials/codex/business:ro
      - ${CODEX_PERSONAL_HOME}:/credentials/codex/personal:ro
    secrets:
      - command-code-goat

secrets:
  command-code-goat:
    file: ./secrets/command-code-goat
```

The one-time setup copies the operator-provided Command Code key into ignored `./secrets/command-code-goat` with mode `0600`. Do not store the key value or its source pathname in `.env`. `.env` contains only host UID/GID and Codex credential-directory paths.

The broker receives provider credentials. Hermes does not. Both containers mount the same socket directory and run the hquota client/broker under the same intended non-root UID.

For the current official Hermes container, the external stack must start the container using the image's default root/s6 entrypoint and pass `HERMES_UID`/`HERMES_GID` as environment variables. It must not pin Compose `user:` or replace the Hermes entrypoint. The Hermes bootstrap remaps its internal user, initializes `/opt/data`, sets the runtime home, and drops privileges before the gateway runs.

## 28. Installing the client and Skill in Hermes

Build the derivative image from the same hquota source revision as the broker:

```sh
docker build --target hermes -t hermes-hquota:local .
```

This puts the same protocol-compatible `hquota` binary on Hermes `PATH` without modifying the base image runtime contract.

The external stack installs or mounts `skills/quota` into the Hermes persistent Skill directory. With the current official Docker image this is under `/opt/data/skills/quota`.

The external stack must mount only the shared `/run/hquota` socket directory into Hermes. Do not mount Codex credentials or the Command Code secret into Hermes.

## 29. Hermes Skill

Create one Skill named `quota`.

The Skill must invoke `hquota --json` at most once per user quota request, require schema version 1, use only normalized facts, explain account-local errors without requesting credentials, and never choose or switch accounts.

The Skill must not call provider endpoints, read credential files, contain API keys, or perform routing policy.

## 30. Tests

Keep tests close to the code and use Rust's built-in test harness. Tests must cover strict config parsing, provider normalization, headroom derivation, aggregation, protocol behavior, timeout handling, redirect rejection, proxy disabling, secret-redaction behavior, and deterministic public JSON. Do not add a snapshot-test framework.

Never commit real access tokens, account IDs, email addresses, or raw private live responses.

## 31. Live acceptance procedure

Run live acceptance only on an operator-controlled machine that already owns the credentials.

```text
1. Start the broker and Hermes stack with 2 Codex accounts and 1 Command Code account.
2. Run hquota doctor.
3. Run hquota and hquota --json from the client side.
4. Ask Hermes to show quota.
5. Inspect mounts and confirm Hermes has no provider credential or Command Code secret mount.
6. Confirm broker and Hermes-side hquota client use the same intended non-root UID.
7. Confirm the shared runtime directory is mode 0700 and the socket is mode 0600.
8. Inspect logs and outputs for secret leakage.
```

Do not save raw provider responses from this procedure.

## 32. Build and quality gates

Required local/CI checks:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked
```

The build must use the committed `Cargo.lock`. No network credential is required for the test suite.

For Docker packaging changes also run, in an operator environment with Docker available:

```sh
docker build --target broker -t hquota-broker:local .
docker build --target hermes -t hermes-hquota:local .
docker compose config --quiet
```

## 33. Change policy

This design does not preserve backward compatibility by default.

For a deliberate breaking machine-contract change, update `spec.md`, increment the affected public/wire version, update producer and consumers together, and delete obsolete compatibility paths unless a real deployment requires overlap.

Provider wire changes isolated inside adapters do not require a public schema change when normalized semantics remain unchanged.

## 34. Non-normative research basis

The implementation decisions above were checked against current public or inspectable sources through 2026-09-17.

Primary and high-value sources include the OpenAI Codex source, Hermes Agent Docker and Skill documentation, Docker Compose secret documentation, and Command Code usage-limit documentation. Provider-internal endpoints remain adapter implementation surfaces rather than hquota public contracts.
