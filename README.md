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

`Dockerfile` has `broker` and `hermes` targets sharing the same compiled binary.
The `hermes` target can extend an existing Hermes image through
`HERMES_BASE_IMAGE`; this is the intended path for integration into a larger
Hermes stack. Compose assigns stable local image names so another Compose project
can reference the images without depending on this repository at runtime.

Run the one-time setup with the two Codex credential directories and the Command
Code key file:

```sh
sh ./compose-setup.sh \
  "$HOME/.codex-business" \
  "$HOME/.codex-personal" \
  "$HOME/.config/command-code/api-key"
```

The script writes `.env`, copies `config.example.json` to the ignored local
`config.json` on first use, and creates `run/hquota` with mode `0700` under the
current non-root UID. Edit `.env` or `config.json` once if local paths, account
names or providers differ.

Build the broker image:

```sh
docker compose config --quiet
docker compose build hquota
```

The resulting image is `hquota-broker:local` by default. Start only the broker:

```sh
docker compose up -d hquota
docker logs hermes-hquota
```

The explicit container name is `hermes-hquota` by default, so Compose does not
append a replica suffix such as `-1`.

To build an Hermes image containing the same `hquota` binary, set the base image
in `.env` when required and build the optional `gateway` service:

```dotenv
HERMES_BASE_IMAGE=hermes-base:hquota
```

```sh
docker compose build gateway
```

The resulting image is `hermes-hquota:local` by default. For a standalone test of
both services from this repository, enable the optional Hermes profile:

```sh
docker compose --profile hermes up -d
```

For an external `hermes-stack`, build both images here and reference only
`hquota-broker:local` and `hermes-hquota:local` there. The external stack should
own its own runtime directory and `config.json`; it does not need
`HQUOTA_SOURCE`, this repository mounted into the broker, or another derivative
Dockerfile just to copy the `hquota` binary.

Do not use UID 0. Ensure the key file and Codex credential files are readable by
the configured broker UID. File-backed Compose secrets do not portably remap
ownership. The broker receives read-only credential mounts and no Hermes data
volume. Hermes receives only its own runtime data, the Skill, and the shared
socket, never provider credentials.

The `quota` Skill is mounted into `$HERMES_HOME/skills/quota` by this repository's
standalone Compose configuration. An external Hermes stack must install or mount
`skills/quota` into its own Hermes data directory. The Skill calls `hquota --json`
once, checks schema version 1, and reports facts without choosing accounts or
routing future work.

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
