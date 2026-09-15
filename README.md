# hquota

Read-only, on-demand Codex and Command Code quota for humans and Hermes.
One Rust binary provides a credential-facing broker and a Unix-socket client.
The governing contracts are [spec.md](docs/spec.md) and [design.md](docs/design.md).

## Build and test

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked
```

Tests use synthetic credentials, local sockets and sanitized synthetic fixtures.
They do not contact providers. Dependency acquisition can require crates.io access.

## Local broker

Install the binary on the broker and client machines/containers. Both must share
one Unix socket and run as the same non-root UID. Linux is the deployment target.
The broker checks ownership and permissions through `/proc/self` and filesystem
metadata. Provision `/run/hquota` with mode `0700`, owned by that UID. The broker
creates `/run/hquota/hquota.sock` with mode `0600`.

Copy `config.example.json` to `/etc/hquota/config.json`, adjusting account names and
absolute credential paths. Configuration contains paths only. The broker rejects
unknown fields, duplicate identities, invalid names and relative credential paths.

```sh
hquota serve --config /etc/hquota/config.json
# In another terminal under the same UID:
hquota
hquota --json
hquota --provider codex
hquota --account business
```

Quota commands exit successfully when a report arrives, even if accounts failed.
Selectors filter that single report locally. `hquota doctor` performs **live**
credential/provider checks and fails if any account fails. Run it only when you
intend to contact providers. Unknown-field warnings alone do not fail doctor.

Codex files require nonempty `tokens.access_token` and `tokens.account_id`.
Command Code files contain one key, with surrounding ASCII whitespace permitted.
Files are read anew per request, never modified or cached. Mount credential
**directories** when atomic file replacement must remain visible in containers.

## Docker Compose

`Dockerfile` has `broker` and `hermes` targets sharing the same compiled binary.
The runtime images use a non-root user. Compose requires explicit UID/GID and
host paths; it does not create a root initialization service.

```sh
export HERMES_UID="$(id -u)" HERMES_GID="$(id -g)"
export HQUOTA_RUNTIME_DIR="/run/user/$(id -u)/hquota"
export HERMES_DATA_DIR="$HOME/.hermes"
install -d -m 0700 "$HQUOTA_RUNTIME_DIR" "$HERMES_DATA_DIR"
# Set CODEX_BUSINESS_HOME and CODEX_PERSONAL_HOME to existing credential directories.
# Set COMMAND_CODE_KEY_FILE to an existing broker-readable key file.
docker compose config --quiet
docker compose build
docker compose up -d
```

Do not use UID 0. Ensure the key file is readable by the configured broker UID.
File-backed Compose secrets do not portably remap ownership. The broker receives
read-only credential mounts and no Hermes data volume. Hermes receives only its
own runtime data, the Skill, and the shared socket, never provider credentials.
The Hermes target runs `hermes gateway run` directly as the non-root user rather
than using the base image's s6 initialization. Configure Hermes and its messaging
platforms in its own data directory before starting the gateway.

The `quota` Skill is mounted into `$HERMES_HOME/skills/quota`. For a non-Compose
installation, copy `skills/quota` into the Hermes skills directory and install
`hquota` on PATH. The Skill calls `hquota --json` once, checks schema version 1,
and reports facts without choosing accounts or routing future work.

## Provider evidence and limits

Provider APIs are unstable. No live response was captured for these fixtures.
Codex DTOs follow the generated models and backend wrapper in
[openai/codex](https://github.com/openai/codex/tree/main/codex-rs/codex-backend-openapi-models/src/models).
Command Code wire evidence comes from
[CodexBar #2466](https://github.com/steipete/CodexBar/pull/2466) and
[pi-commandcode-provider](https://github.com/safzanpirani/pi-commandcode-provider/blob/main/docs/troubleshooting.md).
Only `/alpha/billing/credits` is requested. Both observed rolling-window locations
are recognized; simultaneous pools fail rather than choosing one silently.
Monthly/premium/open-source balances are omitted because the available evidence
does not establish whether they represent remaining or granted amounts.
No currency is inferred. Purchased/free balances remain separate from headroom.

Live Docker/Hermes acceptance requires an operator-controlled environment with
two Codex accounts and one Command Code account. Follow design §31, including
mount inspection and leakage checks. Passing offline tests is not live acceptance.
No OAuth refresh, retry, history, persistence, proxy, account switching or provider
URL configuration is implemented.
