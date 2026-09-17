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

## Local broker

The broker owns provider credentials. Clients only connect to
`/run/hquota/hquota.sock`. Broker and clients must use the same non-root UID;
`/run/hquota` must be owned by that UID with mode `0700`, and the broker creates
the socket with mode `0600`.

```sh
hquota serve --config /etc/hquota/config.json
hquota --json
hquota doctor
```

Credentials are read on demand, never modified or cached.

## Docker Compose

This repository builds only the broker image. Hermes packaging belongs to the
external Hermes stack so hquota cannot override Hermes' `USER`, `ENTRYPOINT`, or
startup contract.

Prepare local paths and the Command Code secret once:

```sh
sh ./compose-setup.sh \
  "$HOME/.codex-business" \
  "$HOME/.codex-personal" \
  "$HOME/.config/command-code/api-key"
```

`compose-setup.sh` creates `run/hquota` and `secrets`, copies the Command Code key
to ignored `secrets/command-code-goat`, copies `config.example.json` to ignored
`config.json` if needed, and generates `.env` with only the host UID/GID and the
two Codex directory paths. No shell exports are required.

Build and run:

```sh
docker compose config --quiet
docker compose build
docker compose up -d

docker compose exec hquota hquota --json
```

The image name is `hquota:local`. Compose does not set a custom container name.

An external Hermes stack should derive its Hermes image from the Hermes base it
already uses and copy only `/usr/local/bin/hquota` from `hquota:local`, for
example:

```dockerfile
FROM hquota:local AS hquota
FROM hermes-base:hquota
COPY --from=hquota /usr/local/bin/hquota /usr/local/bin/hquota
```

The stack then bind-mounts the same host `run/hquota` directory into the broker
and Hermes containers. Provider credential mounts and the Command Code secret
belong only to the broker. The quota Skill is in `skills/quota` and should be
mounted under Hermes' persistent skills directory.

## Provider evidence and limits

Provider APIs are unstable. Codex quota uses the current ChatGPT usage surface;
Command Code uses `/alpha/billing/credits`. Provider wire handling is isolated in
the adapters and normalized into the public schema. No OAuth refresh, retry,
history, persistence, proxy, account switching, or provider URL configuration is
implemented.

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE). Dependencies and container base images retain their own licenses.
