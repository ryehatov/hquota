# Fixture provenance

All values and account names are synthetic. No live provider response or private
credential was captured. `report.json` is the checked-in public schema snapshot.

Codex field names follow openai/codex generated OpenAPI models and the backend
wrapper, inspected during implementation. Relevant source blobs include
`rate_limit_status_payload.rs` (`81f89edbb89361ca2fb209468ba342e9015e4dec`),
`rate_limit_status_details.rs` (`ca9fdfe2406d5d03a557cd3b8018c88abe80476d`),
`rate_limit_window_snapshot.rs` (`b2a6c0c228572521b4e247d2f4f47258add71c8c`),
`credit_status_details.rs` (`b62b88d7159fcf9d828cb63b98c205603f9436e8`),
and backend-client `types.rs` (`6b45c6e20c0a396072b3b772a2338df22509559e`).
These are source blob identifiers, not a pinned repository revision.

Command Code shape follows the synthetic fixture and parser in
<https://github.com/steipete/CodexBar/pull/2466> and the alpha endpoint example at
<https://github.com/safzanpirani/pi-commandcode-provider/blob/main/docs/troubleshooting.md>.
These are third-party observations, not a stable first-party schema. The fixture
combines observed optional fields to exercise normalization. It does not assert
that a particular live subscription returns all of them together.

Tests mutate these fixtures to exercise schema drift, absent windows, numeric
validation and account-local failure. Unknown monthly/premium/open-source balance
semantics are deliberately not represented as public balance facts.
