# MCP troubleshooting

## Tools missing from the list

The tools are absent from the deferred-tools list, with no "failed to connect" message. The session started before `.mcp.json` was written or updated, or before `mct-mcp-server` was built. Restart Codex or Claude Code, or reconnect MCP. Direct SQLite access is a same-session fallback only, and only through this project's own tooling.

## CONNECTION_CLOSED

This is a real connection failure, not a missing list. It usually means `.mct-index/index.sqlite3` was migrated by a branch with more `M::up` entries in `crates/mct-index/src/schema.rs` than the branch you have checked out. The checked-out binary sees a migration number from the future and stops at startup with `migration error: Attempt to migrate a database with a migration number that is too high`.

1. Confirm it with `scripts/unix/mcp-smoke.sh`. It runs the installed server binary, sends a JSON-RPC `initialize` over stdin and prints the startup error.
2. Run `scripts/unix/reinstall.sh --reindex`. It reinstalls the binaries from the checked-out branch and rebuilds `.mct-index/index.sqlite3` for that schema. By hand, delete the file and run `cargo run -p mct-cli -- --root . init`. The index is fully derived from source, so deleting it is safe.
3. Reconnect the MCP client.

An installed binary older than the index fails the same way, for example after you run a newer `target/debug` server against this repo. `scripts/unix/reinstall.sh` covers that too.

The problem comes back whenever you switch between branches with a different migration count without reindexing first. `scripts/unix/install-hooks.sh` installs a `post-checkout` hook that rebuilds the index automatically.
