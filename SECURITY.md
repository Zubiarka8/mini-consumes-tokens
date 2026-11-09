# Security policy

## What mct touches

`mini-consumes-tokens` runs on your machine and reads your source code. It indexes into a local SQLite file under `.mct-index/` and sends nothing to a remote service. The optional `semantic` build downloads an embedding model once and then runs it locally.

The local dashboard (`mct-cli web`) binds to `127.0.0.1` only. It refuses requests with a foreign `Host` or `Origin`, protects every write with a CSRF token, and serves a strict Content Security Policy. It is read-only apart from review decisions and the `.mctignore` and `.gitignore` files.

Parsers never execute the code they read. Known robustness issues found by fuzzing are recorded in [`docs/04-security/advisories/`](docs/04-security/advisories/).

## Reporting a vulnerability

Please report a problem privately first, so it can be fixed before it is public:

1. Open the repository's **Security** tab on GitHub and choose **Report a vulnerability**: https://github.com/Zubiarka8/mini-consumes-tokens/security/advisories/new
2. Describe what you found, the version (`mct-cli --version`), your operating system, and the smallest input or steps that reproduce it.

Do not put exploit details in a public issue. If the private form is not available to you, open an issue that only says you have a security report and asks for a private channel.

You should get a first answer within a week. Fixes are listed in the [changelog](CHANGELOG.md) when they ship.

## Supported versions

Only the latest release gets security fixes. Update with `mct-cli update`.
