# Agent 06 — shareable configuration and documentation

Follow the shared rules. Branch: `codex/audit-config-docs`, after 02. Goal: prevent F01 and fix F09.

F01 is a local condition: configurations contain a token and `.codex/config.toml` was not ignored. Do not read, copy, or edit personal configuration in the original checkout. Prepare a secret-free template, rules that prevent accidental commits of sensitive local configuration, and documentation for credential injection. Do not ignore all of `.codex` indiscriminately if it contains shareable content. Use fake credentials in detection tests. Do not add a large scanning dependency/service without justification.

F09: the example `INSTALL_DIR=... curl ... | bash` applies the variable to `curl`. Correct the assignment for the installer and validate its semantics with a harmless local script and a temporary directory; do not run a remote `curl | bash` to test documentation.

Scope: `.gitignore`, a sanitized new template, README/docs, and a small check if needed. Do not touch Rust functionality, `Cargo.lock`, global authentication, or secrets. The account owner must rotate the real credential; this remains pending. Do not claim that it has been revoked.

Acceptance: usable template, sensitive configuration cannot be added accidentally, custom install directory example works, and rotation instructions distinguish `.mcp.json` from `.codex/config.toml`. Deliver a local commit and proposed PR.
