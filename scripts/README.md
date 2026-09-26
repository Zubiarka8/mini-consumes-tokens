# scripts/

Wrappers for the commands this repo's contributors (and coding agents) run
over and over. Each one prints a short summary and sends the full output to
`target/script-logs/`, so a routine check costs a few lines of terminal
output — or of an agent's context — instead of hundreds.

Scripts are split by operating system, one directory each, with the same
script names and flags wherever both exist:

| Directory | For | Status |
|---|---|---|
| `unix/` | macOS and Linux (bash 3.2+, so macOS's `/bin/bash` works) | the scripts below |
| `windows/` | Windows (PowerShell) | not written yet — see `windows/README.md` |

## unix/

| Script | What it does |
|---|---|
| `check.sh` | CI's `build-test` job locally: `cargo test`, the CI clippy invocation (unwrap/expect/panic denied), the `mct-eval` quality gate. One line per step; on failure, only the failing tests' panic messages, the lint headlines with their location, or the eval regressions. `-p <crate>` for one crate (skips eval), `--only test\|clippy\|eval`, `--no-eval`. |
| `reinstall.sh` | `cargo install` of `mct-mcp-server` and `mct-cli` from this checkout, rebuilds `.mct-index/index.sqlite3` if its schema is newer than the checkout's, then runs `mcp-smoke.sh`. `--reindex` to always rebuild, `--semantic` for `--features semantic`, `--server-only`. Run it after switching branches or merging a server change, then `/mcp`. |
| `mcp-smoke.sh` | Starts the server over stdio, sends `initialize` + `tools/list`, reports the tool count or the real startup error behind Claude Code's `CONNECTION_CLOSED`. `--dev` for a fresh `target/debug` build, `--expect <tool>` to assert a tool is listed, `--list`. |
| `new-tool-check.sh <tool>` | The wiring checklist for a new MCP tool (`#[tool]` method, `tools.ttc`, `KNOWN_TOOL_NAMES`, `TOOL_CATEGORIES`, `batch` dispatch, tests, eval suite, README, CLAUDE.md, protocol spec) with what's still missing, then the catalog tests. `--no-tests` to skip them. |
| `new-language-check.sh <suffix>` | The same for a new `crates/mct-lang-<suffix>` (CONTRIBUTING.md's checklist): `mct-core`/grammar dependencies, `LanguageParser` impl, workspace member and dependency, both `Cargo.toml`s and `build_registry`s, `tests/parse.rs` with a `ParseError::Syntax` case, fuzz harness and its CI matrix entry, README and `internal/checklist.md`. Then the crate's tests and mct-cli's registry tests. `--readme-name 'C++'` when README spells it differently, `--no-tests`. |
| `token-report.sh` | Every token measurement in one screen: MCP tool vs grep+read per language (`token_benchmark`), JSON/TOON/text per response shape (`format_benchmark`), each composite tool vs separate calls (any `mct-mcp-server` test printing `% fewer`), and `mct-eval`'s catalog/response totals. `--markdown <file>` for a PR body, `--no-eval`. |
| `install-hooks.sh` | Installs the git hooks under `hooks/` (copied, so they survive checking out older branches; re-run to update, `--uninstall` to remove). Today one: `post-checkout`, which on a branch switch that changes the index schema's migration count rebuilds `.mct-index/index.sqlite3` if it's now newer than the installed binaries (the `CONNECTION_CLOSED` failure) and says how to serve the branch. Silent otherwise; `MCT_SKIP_HOOKS=1` skips it. |

### Logs

Every log in `target/script-logs/` has a fixed name per script and step,
and the next run of that script overwrites it, so the directory stays at a
few dozen small files and never needs emptying. Deleting it, or
`cargo clean`, is always safe. The scripts create no other temporary files.

Each log starts with a line recording the command, when it ran and which
checkout it ran against, so you can tell an old log from the one you just
produced:

```
# scripts/unix/check.sh -p mct-core — 2026-09-26 19:21:05 +0200 — chore/scripts@9e362dc-dirty
```

`head -n1 target/script-logs/*` shows all of them at once.
`smoke-stdout.jsonl` is the exception: it holds the server's raw JSON-RPC
replies, so it gets no header line (its file date is the only record).
`token-report.md` has the same line as an HTML comment, so it doesn't show
in a PR body.

`lib.sh` holds the shared helpers and refuses to run on anything but macOS
or Linux. Every script takes `--help`; `mcp-smoke.sh` needs `jq` or
`python3`. Exit status is 0 on success, 1 on a failed check, 2 on a usage
error.
