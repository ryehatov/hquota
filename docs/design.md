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

Set:

```text
Accept: application/json
```

where appropriate.

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

Use a minimal private DTO instead of mirroring the complete Codex auth schema:

```rust
#[derive(Deserialize)]
struct CodexAuthFile {
    tokens: Option<CodexTokens>,
}

#[derive(Deserialize)]
struct CodexTokens {
    access_token: String,
    account_id: Option<String>,
}
```

Do not use `deny_unknown_fields` for the provider-owned credential format. New unrelated fields must not break credential parsing.

### 14.2 Request

Fixed request:

```text
GET https://chatgpt.com/backend-api/wham/usage
Authorization: Bearer <access_token>
ChatGPT-Account-Id: <account_id>
Accept: application/json
User-Agent: hquota/<version>
```

Do not follow redirects.

Do not refresh on `401` or `403`.

Map authentication failure to:

```text
authentication_required
```

Map connection/TLS/5xx and other non-auth transport failures to the smallest applicable public error code from `spec.md`.

Do not include the response body in the public error or stderr log.

### 14.3 Wire DTO strategy

The WHAM endpoint is a first-party but unsupported internal surface. Keep its DTOs private to `codex.rs`.

Use tolerant Serde structs:

- do not apply `deny_unknown_fields` in normal wire parsing;
- make provider-optional windows optional;
- model known semantic fields with strict Rust types;
- reject type changes for known fields;
- do not recover a known malformed field from another field by heuristic.

Known version 1 normalization targets include:

- ordinary rate-limit windows;
- additional rate-limit pools when exposed;
- credits/balance state when exposed;
- spend-control state when exposed and unambiguous;
- explicit ordinary-usage availability when exposed;
- provider rate-limit reached reason when exposed.

A response is valid even when only one rate window is present.

Do not require both `5h` and `7d`.

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

If an additional limit has a provider-owned identifier, preserve it through deterministic scope normalization.

Every additional limit is auxiliary in version 1.

If an additional limit object is present but malformed in a known semantic field, fail the account atomically with `upstream_schema_changed`.

## 15. Command Code adapter

### 15.1 Credential read

For each Command Code fetch:

1. open the configured `api_key_file` path read-only;
2. read it as UTF-8 text;
3. trim surrounding ASCII whitespace;
4. reject an empty result or any remaining ASCII whitespace;
5. keep the key only for the request lifetime.

File-not-found maps to `credential_missing`.

Invalid UTF-8 or an empty value maps to `credential_invalid`.

### 15.2 Request

Fixed request:

```text
GET https://api.commandcode.ai/alpha/billing/credits
Authorization: Bearer <api-key>
Accept: application/json
User-Agent: hquota/<version>
```

Do not call `/alpha/billing/subscriptions`, `/alpha/usage/summary`, or other endpoints in version 1.

### 15.3 Normalization

Keep Command Code wire DTOs private to `command_code.rs`.

Normalize only fields whose semantics are established by the current response fixture and provider documentation/research.

Expected semantic categories are:

- `windowLimits.fiveHour` -> primary `rate_window`;
- `windowLimits.weekly` -> primary `rate_window`;
- monthly credits -> auxiliary balance when the response represents a current remaining amount;
- purchased credits -> auxiliary balance;
- free credits -> auxiliary balance.

For rolling windows that expose numeric used and cap values:

```text
used_percent_raw = 100 * used / cap
```

Requirements:

- `cap` must be finite and greater than zero;
- `used` must be finite and non-negative;
- use conservative integer percentage normalization;
- preserve continuous `used_value` and `limit_value` as `f64`;
- use `unit = credits` when the provider endpoint defines the values as credits;
- preserve the provider reset time when available.

Do not hard-code plan caps such as GOAT limits into the domain. Use the returned cap.

If a balance field's meaning is ambiguous between granted, used, or remaining, omit the balance fact rather than guess.

## 16. Provider response atomicity

Provider parsing and normalization for one account must complete before an `AccountQuota` success value is constructed.

Implementation pattern:

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

Any failure before the final step returns:

```text
AccountQuota::error(...)
```

with no scopes and no facts.

Do not return partial facts.

## 17. Broker concurrency

Use blocking threads only.

### 17.1 Connection concurrency

The listener loop should:

```text
accept connection
spawn one std::thread
return immediately to accept
```

Each connection handles exactly one request and exits.

No worker pool is required for version 1.

### 17.2 Account concurrency

For `quota` and live `doctor` operations, fetch accounts concurrently with:

```rust
std::thread::scope
```

Use one scoped thread per configured account.

Join every account thread and restore deterministic configuration order before serialization.

Do not share credential values between account threads.

No mutex is required for report data if each thread returns one owned account result.

## 18. Unix socket lifecycle

Default path:

```text
/run/hquota/hquota.sock
```

### 18.1 Startup

Before binding:

1. verify the parent runtime directory exists;
2. verify it is not unexpectedly permissive;
3. if the socket path exists, first test whether a live broker accepts a connection;
4. if a live broker exists, fail startup;
5. if the existing path is a stale Unix socket, remove it;
6. if the existing path is not a Unix socket, fail startup;
7. bind `UnixListener`;
8. set socket mode `0600` explicitly.

Do not recursively delete runtime-directory contents.

### 18.2 I/O

Set read and write timeouts to 15 seconds on accepted and client-side `UnixStream` values.

Framing:

```text
client writes request JSON
client shutdown(Write)
broker reads to EOF
broker writes response JSON
broker shutdown/close
client reads to EOF
```

Do not append protocol messages after the first response.

## 19. Broker protocol types

Use a small internal envelope.

Request:

```rust
struct Request {
    protocol_version: u32,
    op: Operation,
}
```

Operations:

```text
quota
health
doctor
```

Recommended response envelope:

```json
{
  "protocol_version": 1,
  "status": "ok",
  "payload": { }
}
```

Protocol errors use:

```json
{
  "protocol_version": 1,
  "status": "error",
  "error": {
    "code": "protocol_version_mismatch"
  }
}
```

Protocol error codes are internal and do not belong to the public `QuotaReport` schema.

The client must verify exact protocol version before interpreting `payload`.

For a `quota` operation, the payload is a `QuotaReport`.

For `health`, use only process-local state:

```json
{
  "status": "ok",
  "configured_accounts": 3
}
```

`health` must not read credentials or contact providers.

## 20. Doctor implementation

`doctor` performs live diagnostics.

For every configured account, test:

```text
credential file exists
credential minimal fields parse
provider request succeeds
known semantic fields parse
normalization invariants hold
unknown additive fields are reported when detectable
```

Suggested internal diagnostic record:

```rust
struct Diagnostic {
    provider: Provider,
    account: String,
    severity: Severity,
    code: DiagnosticCode,
}
```

Keep diagnostic codes machine-oriented internally even if version 1 exposes only human `hquota doctor` output.

Do not include arbitrary provider messages.

Exit behavior follows `spec.md`.

## 21. Unknown-field diagnostics

Normal Serde deserialization ignores unknown fields, so `doctor` needs a separate low-cost detection path if unknown-field reporting is implemented.

Preferred implementation:

1. parse the response once into `serde_json::Value`;
2. inspect known object keys against small static key sets;
3. record unknown keys as warnings;
4. deserialize the same `Value` into the typed wire DTO;
5. normalize normally.

Do this only in `doctor` mode.

Normal quota requests should deserialize directly into typed DTOs and avoid the extra inspection logic.

Do not build a generic JSON-schema validator.

## 22. Human renderer

The renderer consumes only `QuotaReport`.

It must not access provider DTOs or credentials.

Suggested ordering:

```text
provider order: configuration-derived first appearance
account order: configuration order
scope order: primary, auxiliary, unknown
rate windows: canonical 5h, canonical 7d, then other durations
other facts: balance, spend limit, availability
```

Use a 10-cell bar:

```text
filled_cells = round(remaining_percent / 10)
```

The bar is presentation-only and does not change domain values.

Example:

```text
Quota · 2026-09-15 09:00 UTC

Codex
  business
    5h  ████████░░ 78% left · reset 11:43 UTC
    7d  █████░░░░░ 52% left · reset 2026-09-19 18:00 UTC
    headroom 52%
```

Do not add a TUI dependency.

## 23. Client-side filters

The client always requests one complete `QuotaReport` for the invocation.

Then apply:

```text
--provider
--account
```

locally before rendering or serializing public output.

`--account NAME` without `--provider` may match more than one provider if the same account name exists in different providers. In that case, return all matching `(provider, account)` pairs. This preserves the specification that the stable identity is the pair, not the account name alone.

An unknown selector that matches no configured report entry should produce an empty filtered report and exit `0`; it is not a broker failure.

## 24. Logging

Use direct `eprintln!` calls for a small set of operational events.

Centralize account-failure formatting so only public error codes are emitted.

Good:

```text
hquota: codex/business fetch failed: authentication_required
```

Forbidden:

```text
hquota: request failed: 401 body={...}
hquota: Authorization: Bearer ...
hquota: parsed auth = CodexCredential { ... }
```

Do not derive `Debug` on secret-bearing types.

Do not print raw `reqwest` errors if they can contain request URLs with future sensitive query parameters. Map them to internal categories first.

## 25. Error mapping

Use one provider-independent account error enum matching `spec.md`.

Suggested mapping:

```text
credential file ENOENT           -> credential_missing
credential parse/shape invalid    -> credential_invalid
HTTP 401/403                      -> authentication_required
reqwest timeout                   -> timeout
DNS/TLS/connect/5xx/other HTTP    -> upstream_unavailable
known provider field incompatible -> upstream_schema_changed
normalization invariant failure   -> upstream_schema_changed
```

Do not automatically retry any category.

If an account-fetch thread panics, the broker should convert that account to `upstream_unavailable` or fail the whole broker request with a sanitized internal error. Tests should make panics unreachable in normal parsing paths.

## 26. Docker images

Use one multi-stage Dockerfile with a shared Rust build and two runtime targets.

The broker target uses a minimal runtime:

```dockerfile
FROM debian:bookworm-slim AS broker
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/hquota /usr/local/bin/hquota
USER 10000:10000
ENTRYPOINT ["hquota"]
CMD ["serve"]
```

The exact base image may change. The broker target requires:

- non-root runtime;
- CA certificates available for HTTPS;
- no compiler toolchain in the runtime image;
- only the required executable and runtime files.

The Hermes target is a derivative client image:

```dockerfile
ARG HERMES_BASE_IMAGE=nousresearch/hermes-agent:latest
FROM ${HERMES_BASE_IMAGE} AS hermes
COPY --from=build /src/target/release/hquota /usr/local/bin/hquota
```

Do not override `USER`, `ENTRYPOINT`, or `CMD` in the Hermes target. The current official Hermes image starts its s6 bootstrap as root, uses `HERMES_UID`/`HERMES_GID` to remap the internal runtime user, initializes `/opt/data`, sets the runtime home, and then drops privileges. The derivative image must preserve that contract.

Do not copy the quota Skill to an arbitrary image path. An external Hermes stack installs or mounts the Skill into the persistent Hermes Skill directory.

## 27. Docker Compose integration

The repository Compose file owns only the broker. An external Hermes stack owns the Hermes container and adjacent services such as SearXNG.

Use a host runtime directory rather than a named volume that requires a root initialization helper. The repository convention is:

```text
./run/hquota
```

Before broker startup, it must be owned by the intended non-root UID and have exact mode `0700`.

Broker Compose configuration:

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

Then configure:

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
      "provider": "codex",
      "name": "personal",
      "auth_json": "/credentials/codex/personal/auth.json"
    },
    {
      "provider": "command-code",
      "name": "goat",
      "api_key_file": "/run/secrets/command-code-goat"
    }
  ]
}
```

The one-time setup copies an operator-provided Command Code key source into ignored `./secrets/command-code-goat` with mode `0600`. Do not store the key value or its source pathname in `.env`. `.env` contains only host UID/GID and Codex credential-directory paths.

Docker Compose mounts the Command Code secret only into the broker. The Hermes service must not list that secret or any provider credential mount.

The broker and Hermes-side client must use the same intended non-root UID for the shared socket. For the current official Hermes image, the external stack achieves this by starting Hermes with the image's default root/s6 entrypoint and passing `HERMES_UID` and `HERMES_GID` as environment variables. Do not set Compose `user:` on Hermes and do not replace its entrypoint.

## 28. Installing the client in Hermes

Build one derivative Hermes image for steady state from the same hquota source revision as the broker:

```sh
docker build --target hermes -t hermes-hquota:local .
```

The derivative image adds only `/usr/local/bin/hquota` and preserves the base Hermes runtime contract. This eliminates broker/client protocol version skew without taking ownership of Hermes process supervision.

The external stack mounts the shared socket directory at `/run/hquota` and installs or mounts `skills/quota` into the persistent Hermes Skill directory. With the current official Docker image this directory is under `/opt/data/skills`.

Development MAY bind-mount a local binary instead.

## 29. Hermes Skill

Create one Skill named `quota`.

The Skill should state:

```text
When the user asks about current Codex or Command Code quota:
1. invoke `hquota --json` through Hermes `terminal` once;
2. read schema_version and fail visibly if unsupported;
3. use only the returned normalized facts;
4. explain account-local errors without requesting credentials;
5. compare headroom only where both accounts expose comparable primary headroom;
6. do not select or switch accounts.
```

The Skill should not call provider endpoints itself.

The Skill should not read mounted credential files.

The Skill should not contain API keys or credential setup logic.

A Skill is the correct Hermes integration because the capability is an external CLI that can be invoked through the terminal and does not require Python integration in Hermes.

## 30. Tests

Keep tests close to the code. Use Rust's built-in test harness.

### 30.1 Config tests

Test:

- valid empty config;
- valid mixed-provider config;
- unknown top-level field;
- unknown account field;
- wrong provider-specific credential field;
- relative credential path;
- invalid account name;
- duplicate `(provider, name)`.

### 30.2 Codex fixture tests

Commit sanitized provider fixtures that cover:

- 5-hour + 7-day windows;
- only one window present;
- unknown duration;
- additive unknown top-level field;
- additive unknown known-object field;
- additional rate-limit pool;
- credit balance when exposed;
- explicit availability when exposed;
- malformed known percentage type;
- malformed known reset timestamp.

Never commit real access tokens, account IDs, email addresses, or raw private live responses.

### 30.3 Command Code fixture tests

Cover:

- 5-hour + weekly window;
- fractional used/cap values;
- returned cap rather than hard-coded plan cap;
- monthly balance when unambiguous;
- purchased/free balances when unambiguous;
- additive unknown fields;
- malformed used or cap;
- zero/negative cap.

### 30.4 Pure normalization tests

Test:

```text
30.0% used -> 30 / 70
30.1% used -> 31 / 69
100% used  -> 100 / 0
18000 sec  -> 5h
604800 sec -> 7d
900 sec    -> null canonical
```

Test headroom using one and multiple primary windows.

Test that auxiliary windows do not affect headroom.

### 30.5 Aggregation tests

Given three accounts where one fails:

```text
codex/business -> ok
codex/personal -> authentication_required
command-code/goat -> ok
```

verify:

- three account entries remain;
- the two successes keep their facts;
- the error has no facts;
- normal quota command exits `0`.

### 30.6 Protocol tests

Test:

- quota request/response;
- health request/response;
- doctor request/response;
- exact protocol version success;
- mismatched protocol version failure;
- one-request/one-response EOF behavior;
- client I/O timeout behavior.

Use a temporary Unix socket path under the test temporary directory.

### 30.7 Security tests

At minimum test:

- redirect response is not followed;
- HTTP client has proxy auto-discovery disabled;
- secret-bearing types are never formatted by normal error paths;
- 401 body content does not reach JSON/log output;
- raw malformed provider body does not reach JSON/log output;
- credential file content does not reach `doctor` output.

### 30.8 Snapshot tests

Do not add a snapshot-test crate.

Serialize representative `QuotaReport` values and compare them to checked-in JSON strings or fixture files with normal assertions.

This verifies the public machine contract without another dependency.

## 31. Live acceptance procedure

Run live acceptance only on an operator-controlled machine that already owns the credentials.

Procedure:

```text
1. Start Compose with 2 Codex accounts and 1 Command Code account.
2. Run hquota doctor.
3. Run hquota.
4. Run hquota --json.
5. Run hquota --provider codex.
6. Run hquota --account business.
7. Ask Hermes: Show all quota.
8. Ask Hermes: Which Codex account has more primary quota headroom?
9. Inspect container mounts and confirm Hermes has no provider credential mount.
10. Inspect broker logs and outputs for secret leakage.
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

The build must use the committed `Cargo.lock`.

No network credential is required for the test suite.

For Docker packaging changes, also run in an operator environment with Docker available:

```sh
docker build --target broker -t hquota-broker:local .
docker build --target hermes -t hermes-hquota:local .
docker compose config --quiet
```

## 33. Change policy

This design does not preserve backward compatibility by default.

For a deliberate breaking machine-contract change:

1. update `spec.md`;
2. increment public `schema_version` if JSON changes incompatibly;
3. increment broker `protocol_version` if broker/client wire changes incompatibly;
4. update broker, client, and Hermes Skill together;
5. delete obsolete compatibility paths instead of retaining them unless a real deployment requires overlap.

Provider wire changes that can be contained inside `codex.rs` or `command_code.rs` do not require a public schema change when normalized semantics remain unchanged.

## 34. Non-normative research basis

The implementation decisions above were checked against current public or inspectable sources on 2026-09-17.

Primary and high-value sources:

- OpenAI Codex source, current `auth.json` representation:  
  `https://github.com/openai/codex/blob/main/codex-rs/login/src/auth/storage.rs`  
  `https://github.com/openai/codex/blob/main/codex-rs/login/src/token_data.rs`
- OpenAI Codex app-server quota schema, used as a semantic reference even though version 1 acquires quota directly:  
  `https://github.com/openai/codex/blob/main/codex-rs/app-server-protocol/schema/json/v2/GetAccountRateLimitsResponse.json`
- OpenAI Codex backend client, current ChatGPT rate-limit implementation and credential-header handling:  
  `https://github.com/openai/codex/blob/main/codex-rs/backend-client/src/client.rs`
- Hermes Agent Docker setup and Skill guidance:  
  `https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/docker.md`  
  `https://github.com/NousResearch/hermes-agent/blob/main/website/docs/developer-guide/creating-skills.md`
- Docker Compose secrets:  
  `https://docs.docker.com/compose/how-tos/use-secrets/`  
  `https://docs.docker.com/reference/compose-file/secrets/`
- Command Code usage-limit semantics:  
  `https://commandcode.ai/docs/resources/usage-limits`

The Command Code `/alpha/billing/credits` machine endpoint is not treated as a documented stable public API. Its use in version 1 is based on current third-party observations and must remain isolated inside `command_code.rs`. Representative references include:

- `https://github.com/steipete/CodexBar/issues/2629`
- `https://github.com/safzanpirani/pi-commandcode-provider/blob/main/docs/troubleshooting.md`

Similarly, the Codex `https://chatgpt.com/backend-api/wham/usage` route is a first-party current implementation surface but is not treated as the hquota public contract. Provider drift is therefore an expected adapter-maintenance event.
