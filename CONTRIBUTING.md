# Contributing

Open an issue for a bug or proposed change, or submit a focused pull request.
Include reproduction steps and expected behavior. Never include real credentials,
provider responses, authorization headers or private account data.

Read [spec.md](docs/spec.md), [design.md](docs/design.md) and
[AGENTS.md](AGENTS.md) before changing behavior. Update the governing document
when intentionally changing a contract. Keep tests deterministic and offline;
use synthetic credentials and sanitized synthetic fixtures only.

Run the local checks before submitting:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --locked
```

Do not run live provider checks or `hquota doctor` against real credentials as
part of routine testing. See [verification.md](docs/verification.md) for the
limits of offline verification.

Contributions are provided under the project's [MIT license](LICENSE).
For security issues, follow [SECURITY.md](SECURITY.md) instead of opening a
public issue.
