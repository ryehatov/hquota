# Project Instructions

`spec.md` and `design.md` are the project single sources of truth.

- `spec.md` defines product behavior, domain semantics, invariants, security boundaries, public contracts, and acceptance criteria.
- `design.md` defines the Rust implementation, provider adapters, protocol, deployment, dependencies, and verification strategy.
- This file defines how to work on the repository. Do not duplicate the specifications here.

If code and documentation disagree, the specifications win unless the task explicitly changes them. Update the governing specification with any intentional contract or architecture change.

## Read before editing

- Read the relevant `spec.md` sections before changing quota semantics, public JSON, CLI behavior, error behavior, security boundaries, or Hermes behavior.
- Read the relevant `design.md` sections before changing Rust structure, provider acquisition, socket protocol, HTTP behavior, configuration, dependencies, Docker integration, or tests.
- Read both before a cross-cutting change.

Inspect the affected code path and nearby tests before editing. Do not infer current behavior from old proposals or superseded designs.

## Development cycle

1. Identify the smallest affected contract and implementation boundary.
2. Make the smallest direct change that satisfies the specifications.
3. Prefer existing code, the Rust standard library, and existing dependencies.
4. Add or update the smallest deterministic test that would detect a regression in changed non-trivial behavior.
5. Run focused checks while iterating, then run the full local gate before handoff.

Do not add abstractions, compatibility layers, configuration knobs, dependencies, or background machinery for hypothetical future use.

## Verification gate

For non-trivial code changes, run:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

Default tests must be deterministic and offline. Use sanitized fixtures and mocks for provider behavior.

Do not run live provider checks, `hquota doctor` against real credentials, or other credential-bearing network tests unless the task explicitly requires local live acceptance. Live responses must not become fixtures or artifacts.

If a required check fails or cannot run, report the exact blocker. Do not silently skip it.

## Security and repository rules

- Preserve unrelated local changes. Do not reset, clean, rebase, rewrite history, or discard user work for convenience.
- Do not commit, push, create remote branches or PRs, or otherwise mutate remotes unless explicitly requested.
- Never place access tokens, API keys, credential files, provider raw responses, private account data, or authorization headers in source, fixtures, snapshots, logs, errors, or prompts.
- Keep credential handling inside the broker boundary. Do not move provider credentials or provider HTTP logic into Hermes.
- Do not refresh, modify, persist, or cache credentials.
- Preserve the fixed-origin, HTTPS-only, no-redirect, no-proxy outbound policy unless the governing specification changes.
- Treat provider schema parsing, socket permissions, config validation, and secret redaction as trust-boundary code. Do not weaken these checks for convenience.

## Scope discipline

Implement only the current task and the requirements in `spec.md` and `design.md`.

In particular, do not add a provider framework, async runtime, persistence, history, polling, caching, account switching, automatic routing, or policy engine unless the specifications are explicitly changed first.

Breaking changes are acceptable when they simplify or correct the design. When a public JSON or broker protocol contract changes, update its version and all affected consumers together rather than maintaining backward compatibility by default.
