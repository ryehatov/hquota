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
They do not contact providers.

## Local broker

The broker and client must share `/run/hquota/hquota.sock` and run as the same
non-root UID. The runtime directory must be owned by that UID with mode `0700`;
the broker creates the socket with mode `0600`.

Copy `config.example.json` to `/etc/hquota/config.json`, adjust account names and
absolute credential paths, then run:

```sh
hquota serve --config /etc/hquota/config.json
hquota --json
hquota doctor
```

Credentials are read per request, never modified or cached.

## Docker Compose

The repository Dockerfile builds one hquota image. It does not build or configure
Hermes or any other consumer image.

One-time setup:

```sh
sh ./compose-setup.sh \
  "$HOME/.codex-business" \
  "$HOME/.codex-personal" \
  "$HOME/.config/command-code/api-key"
```

The script:

- records the current non-root UID/GID and the two Codex directories in `.env`;
- creates `run/hquota` with mode `0700`;
- copies the Command Code key to ignored `secrets/command-code-goat` with mode `0600`;
- copies `config.example.json` to ignored `config.json` on first use.

No shell exports are required afterwards.

```sh
docker compose config --quiet
docker compose up -d --build
docker compose exec hquota hquota --json
```

The local image name is `hquota:local`. Compose intentionally has no custom
container name and owns only the hquota broker.

External deployments may reuse the same `/usr/local/bin/hquota` executable from
`hquota:local` and mount the same Unix-socket directory into a client container.
Those deployments own their own image, process, network, and lifecycle settings.
hquota does not override another image's user, entrypoint, command, or runtime
initialization.

Provider credentials belong only in the broker. A client container receives only
the hquota executable and shared socket.

## Hermes Skill

The `quota` Skill is in `skills/quota`. It calls `hquota --json` once, checks
schema version 1, and reports normalized facts. It does not read credentials,
call providers directly, choose accounts, or switch routing.

## Provider evidence and limits

Provider APIs are unstable. Codex DTOs follow current OpenAI Codex implementation
surfaces. Command Code support uses `/alpha/billing/credits` based on current
observed behavior. Provider drift remains isolated inside the provider adapters.

No OAuth refresh, retry, history, persistence, proxy, account switching or
provider URL configuration is implemented.

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE). Dependencies and container base images retain their own licenses.
