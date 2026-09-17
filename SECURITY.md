# Security

## Reporting a vulnerability

Use the repository's **Security → Advisories → Report a vulnerability** button
on GitHub for a private report when private vulnerability reporting is enabled.
If the button is unavailable, open an issue asking the maintainer to enable
private reporting. Do not include vulnerability details in that public issue.

Describe the affected revision, impact and reproduction steps using synthetic
data. Never send access tokens, API keys, credential files, raw provider
responses, authorization headers or private account data, even in private reports.
If a credential was exposed, revoke or rotate it with the provider.

## Scope and maintenance

Security fixes target the latest code on the default branch. There is no
commitment to backport fixes to older versions.

The broker reads credentials; Hermes must receive only the shared socket and
normalized quota data. Credential handling, provider parsing, socket permissions
and outbound HTTP restrictions are security boundaries. See
[spec.md](docs/spec.md) and [design.md](docs/design.md) for their contracts.
