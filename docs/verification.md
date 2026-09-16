# Local verification

Verified on 2026-09-15 without real provider credentials or provider requests.

- `cargo fmt --check` passed.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo test --locked` passed, with 23 top-level tests and a proxy-environment subprocess check.
- `cargo build --release --locked` passed.
- `git diff --check` passed.
- Compose configuration validated with synthetic absolute host paths.
- Both Docker targets built successfully. The broker build initially exceeded
  a 200-second base-image download timeout; a 600-second retry succeeded.
- The non-root, read-only broker container served health, quota and doctor over
  a `0600` socket in a `0700` directory, using an empty account registry.
- A second container check used the three-account example registry with no
  credential mounts. Quota exited 0 and preserved all three `credential_missing`
  results. Provider/account filters returned only the selected result. Doctor
  exited nonzero and printed only normalized diagnostic codes.
- The Hermes image contains an executable client and the quota Skill. Its
  foreground gateway command help was verified without starting a live gateway.

The test fixtures are synthetic. See `tests/fixtures/README.md` for provenance.
HTTP tests use local plaintext servers only through a test-only HTTPS override;
production construction remains HTTPS-only with fixed provider origins.
Proxy tests use a subprocess environment rather than mutating global process
variables in parallel tests. Redirect and error-body checks exercise the shared
HTTP builder and response mapper.

Not claimed: live provider compatibility, authenticated Hermes conversations,
or full three-account live acceptance under design §31. Those checks require
operator-controlled credentials and were expressly excluded from this task.

Follow-up verification on 2026-09-16 covered the Codex auxiliary scope-ID
collision fix. Independent review approved the corrected allocator and regression
tests. All four Rust gates and `git diff --check` passed again. The broker image
was rebuilt from the current source and exercised with networking disabled,
a read-only root filesystem, no capabilities and a non-root UID. The example
three-account registry, without credential mounts, confirmed socket permissions,
isolated missing-credential results, provider/account filters, an empty unknown
selector result and doctor's nonzero failure status.

Two acceptance-harness attempts were corrected before that successful check.
Mounting the host-built binary into Debian failed due to its newer glibc
requirement; the Dockerfile-built binary was used instead. A synthetic config
used an invalid `codex_home` key; the checked-in example with `auth_json` replaced
it. Neither attempt required a product-code change.
