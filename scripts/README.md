# scripts/

Wrappers for the commands this repo's contributors (and coding agents) run
over and over. Each one prints a short summary and sends the full output to
`target/script-logs/`, so a routine check costs a few lines of terminal
output — or of an agent's context — instead of hundreds.

| Script | What it does |
|---|---|
| `check.sh` | CI's `build-test` job locally: `cargo test`, the CI clippy invocation (unwrap/expect/panic denied), the `mct-eval` quality gate. One line per step; on failure, only the failing tests' panic messages, the lint headlines with their location, or the eval regressions. `-p <crate>` for one crate (skips eval), `--only test\|clippy\|eval`, `--no-eval`. |
| `reinstall.sh` | `cargo install` of `mct-mcp-server` and `mct-cli` from this checkout, rebuilds `.mct-index/index.sqlite3` if its schema is newer than the checkout's, then runs `mcp-smoke.sh`. `--reindex` to always rebuild, `--semantic` for `--features semantic`, `--server-only`. Run it after switching branches or merging a server change, then `/mcp`. |
| `mcp-smoke.sh` | Starts the server over stdio, sends `initialize` + `tools/list`, reports the tool count or the real startup error behind Claude Code's `CONNECTION_CLOSED`. `--dev` for a fresh `target/debug` build, `--expect <tool>` to assert a tool is listed, `--list`. |
| `new-tool-check.sh <tool>` | The wiring checklist for a new MCP tool (`#[tool]` method, `tools.ttc`, `KNOWN_TOOL_NAMES`, `TOOL_CATEGORIES`, `batch` dispatch, tests, eval suite, README, CLAUDE.md, protocol spec) with what's still missing, then the catalog tests. `--no-tests` to skip them. |

Every script takes `--help`. They need bash (3.2+, so macOS's `/bin/bash`
works; Git Bash on Windows), and `mcp-smoke.sh` needs `jq` or `python3`.
Exit status is 0 on success, 1 on a failed check, 2 on a usage error.
