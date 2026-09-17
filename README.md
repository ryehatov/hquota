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

Install the binary on the broker and client machines or containers. Both must
share one Unix socket and run as the same non-root UID. Linux is the deployment
target. The broker requires `/run/hquota` to be owned by its UID with mode
`0700`; it creates `/run/hquota/hquota.sock` with mode `0600`.

Copy `config.example.json` to `/etc/hquota/config.json`, adjust account names and
absolute credential paths, then run:

```sh
hquota serve --config /etc/hquota/config.json
# In another terminal under the same UID:
hquota
hquota --json
hquota --provider codex
hquota --account business
```

`hquota doctor` performs live credential/provider checks. Run it only when live
provider access is intended.

Codex files require nonempty `tokens.access_token` and `tokens.account_id`.
Command Code files contain one key. Credential files are read for each request;
they are never modified or cached.

## Docker images

`Dockerfile` has two runtime targets that share the same compiled binary:

- `broker`: minimal non-root hquota broker image;
- `hermes`: a derivative Hermes image that only adds `/usr/local/bin/hquota`.

The `hermes` target deliberately preserves the base Hermes image's `USER`,
`ENTRYPOINT`, and `CMD`. Current Hermes images start their s6 bootstrap as root,
apply `HERMES_UID`/`HERMES_GID`, initialize `/opt/data`, set the runtime home, and
then drop privileges. An external Compose stack must not pin `user:` or replace
the Hermes entrypoint.

Build stable local images for `hermes-stack`:

```sh
docker build --target broker -t hquota-broker:local .
docker build --target hermes -t hermes-hquota:local .
```

To extend a different Hermes base image, pass it only at build time:

```sh
docker build \
  --build-arg HERMES_BASE_IMAGE=hermes-base:hquota \
  --target hermes \
  -t hermes-hquota:local \
  .
```

## Standalone broker Compose

The repository Compose file owns only the broker. It is also a reference for the
broker service embedded by an external Hermes stack.

Run the one-time setup with the two Codex credential directories and a Command
Code key source file:

```sh
sh ./compose-setup.sh \
  "$HOME/.codex-business" \
  "$HOME/.codex-personal" \
  "$HOME/.config/command-code/api-key"
```

The script:

- writes `.env` with only the host UID/GID and Codex credential-directory paths;
- copies `config.example.json` to ignored `config.json` on first use;
- creates `run/hquota` with mode `0700`;
- copies the Command Code key to ignored `secrets/command-code-goat` with mode
  `0600`.

The Command Code key value and source pathname are not stored in `.env`.
Compose mounts only `secrets/command-code-goat` into the broker as
`/run/secrets/command-code-goat`.

```sh
docker compose config --quiet
docker compose build hquota
docker compose up -d hquota
docker logs hermes-hquota
```

The broker image name is `hquota-broker:local` and its explicit container name
is `hermes-hquota`.

## External Hermes stack

An external `hermes-stack` should own the Hermes container and any SearXNG or
other services. It should:

- use `hermes-hquota:local` for Hermes and `hquota-broker:local` for the broker;
- pass `HERMES_UID` and `HERMES_GID` to Hermes as environment variables;
- not set `user:` or override the Hermes entrypoint;
- bind the same host `run/hquota` directory to `/run/hquota` in both containers;
- mount provider credentials and the Command Code secret only into the broker;
- install or mount `skills/quota` into the Hermes persistent Skill directory
  (for the official image, under `/opt/data/skills`);
- keep `run/hquota` mode `0700` and owned by the shared intended non-root UID.

The Hermes process receives only normalized quota data over the Unix socket. It
must never receive provider credential mounts.

The quota Skill calls `hquota --json` once, checks schema version 1, and reports
facts without choosing accounts or routing future work.

## Provider evidence and limits

Provider APIs are unstable. No live response was captured for these fixtures.
Codex DTOs follow the generated models and backend wrapper in
[openai/codex](https://github.com/openai/codex/tree/main/codex-rs/codex-backend-openapi-models/src/models).
Command Code wire evidence comes from
[CodexBar #2466](https://github.com/steipete/CodexBar/pull/2466) and
[pi-commandcode-provider](https://github.com/safzanpirani/pi-commandcode-provider/blob/main/docs/troubleshooting.md).
Only `/alpha/billing/credits` is requested. Both observed rolling-window
locations are recognized; simultaneous pools fail rather than choosing one
silently. Monthly/premium/open-source balances are omitted because the available
evidence does not establish whether they represent remaining or granted amounts.
No currency is inferred. Purchased/free balances remain separate from headroom.

Live Docker/Hermes acceptance requires an operator-controlled environment with
two Codex accounts and one Command Code account. Follow design §31, including
mount inspection and leakage checks. Passing offline tests is not live
acceptance. No OAuth refresh, retry, history, persistence, proxy, account
switching or provider URL configuration is implemented.

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) for development checks and
[SECURITY.md](SECURITY.md) for private vulnerability reporting.

## License

[MIT](LICENSE). Dependencies and container base images retain their own licenses.
