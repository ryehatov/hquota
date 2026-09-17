# hquota

On-demand Codex and Command Code quota for humans and Hermes.
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

`Dockerfile` has `broker` and `hermes` targets that share one compiled `hquota`
binary. `compose.yaml` intentionally owns only the broker service. An external
Hermes stack should own its Hermes container and mount the shared socket.

Run the one-time setup with the two Codex credential directories and the Command
Code key file:

```sh
sh ./compose-setup.sh \
  "$HOME/.codex-business" \
  "$HOME/.codex-personal" \
  "$HOME/.config/command-code/api-key"
```

The script writes `.env`, copies `config.example.json` to ignored `config.json` on
first use, and creates `run/hquota` with mode `0700` under the current non-root
UID. The generated `.env` contains only host-dependent values required by Compose:
UID/GID and credential paths.

Build and start the broker:

```sh
docker compose config --quiet
docker compose build hquota
docker compose up -d hquota
docker logs hermes-hquota
```

The broker image is `hquota-broker:local` and the container name is
`hermes-hquota`. Both are fixed in `compose.yaml`; they are not environment
configuration.

To build an Hermes image containing the same `hquota` binary:

```sh
docker build --target hermes -t hermes-hquota:local .
```

If Hermes must extend another local base image, pass it only at build time:

```sh
docker build \
  --build-arg HERMES_BASE_IMAGE=hermes-base:hquota \
  --target hermes \
  -t hermes-hquota:local \
  .
```

For an external `hermes-stack`, reference `hquota-broker:local` for the broker and
`hermes-hquota:local` when the derivative Hermes image is needed. The external
stack does not need `HQUOTA_SOURCE`, a mounted hquota source tree, or a second
Dockerfile that copies the `hquota` binary.

Do not use UID 0. Ensure the key file and Codex credential files are readable by
the configured broker UID. File-backed Compose secrets do not portably remap
ownership. The broker receives read-only credential mounts and never receives
Hermes data. Hermes should receive only its own data, the quota Skill, and the
shared socket, never provider credentials.

The `quota` Skill is in `skills/quota`. An external Hermes stack must install or
mount it into the Hermes skills directory. The Skill calls `hquota --json` once,
checks schema version 1, and reports facts without choosing accounts or routing
future work.

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

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) for development checks and
[SECURITY.md](SECURITY.md) for private vulnerability reporting.

## License

[MIT](LICENSE). Dependencies and container base images retain their own licenses.
