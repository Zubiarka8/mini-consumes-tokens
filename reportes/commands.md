# Report: docs-commands

Branch `claude/docs-commands`, base 2641f50. No local skills directory exists in the worktree (`.claude/skills` absent); no skill used.

## What was researched

- `mct-cli --help` and every subcommand's `--help` (existing `target/debug/mct-cli`, version 0.2.0; source tree has no crate changes versus base).
- `mct-mcp-server --help`.
- MCP tool catalog via `discover_tool_categories` and `get_tool_schema` (search_symbols, hybrid_search, build_context_pack, batch, find_dead_code, get_project_overview, get_indexing_status).
- Every `scripts/unix/*.sh` (`--help` or header comment), the `scripts/windows/*.ps1` set, `scripts/windows/README.md`, `scripts/pr-conflicts.py` argparse, `scripts/usage-check.py` header.

## Findings

| File | Claim | Evidence | Action |
|---|---|---|---|
| COMMANDS.md | No `probe` subcommand section | `mct-cli --help` lists `probe`; `probe --help` output | Added section with verbatim help |
| COMMANDS.md | `mct-mcp-server` has only `--root`, `--reindex-debounce-ms` | `--help` shows `--rebuild-embeddings` | Added flag section and updated help block |
| COMMANDS.md | Tool list omits `search_symbols`, `hybrid_search`, `build_context_pack`, `batch` | `discover_tool_categories` | Documented each with params from schemas |
| COMMANDS.md | `limit` default 50, `offset` on all tools | Schemas: search tools default 10, clamp 100; `hybrid_search` uses `top_k`; `list_symbols`/`find_symbol` have no `offset` | Reworded |
| COMMANDS.md | `get_indexing_status` params | Schema has `dependency` | Added |
| COMMANDS.md | `get_project_overview` params | Schema has `max_symbols_per_module`, `path`, `language` | Added |
| COMMANDS.md | `format: toon` tool list | Schemas of search_symbols, hybrid_search, build_context_pack accept it | Added to list |
| COMMANDS.md | README anchor `#4-installation-and-setup` for "Connecting" | Heading is 4.3 | Pointed to `#43-connecting-...` |
| COMMANDS.md | All other subcommand help blocks (init, reindex, status, mcp-register, ignore-init, gitignore-init, dead-code, update, top-level) | Identical to current output | No change |
| scripts/README.md | `mcp-smoke.sh` flags | Script help also has `--bin PATH --root DIR` | Added |
| scripts/README.md | Windows set lists 9 scripts | 15 `.ps1` twins exist (missing from list: pr-body, branch-worktree, pr-conflicts, ci-failures, pr-status, corpus-progress-check) | Listed all; unix-only remain branches-status, publish-branch, release (verified) |
| scripts/README.md | `scripts/pr-conflicts.py`, `usage-check.py` undocumented | Files exist | Added "Helper programs" table |
| scripts/README.md | All 18 unix scripts and their flags | Help/headers match | No change |

## Changes

- 7b4230d `docs(commands)`: COMMANDS.md.
- Next commit `docs(scripts)`: scripts/README.md (heading "Installer regression" renamed "Regression tests" since it also covers `test_pr_conflicts.py`).
- reporte.md committed last.

## Unverified

- Claims in COMMANDS.md about `get_file_tree` pruning `target`, `node_modules`, `.git` and about `find_references`/`find_calls`/`find_callers`/`impact_analysis` schemas (depth/format) were not re-checked.
- `hybrid_search` lexical fallback without a model comes from the tool description, not run.
- Windows `.ps1` flag parity was checked via `scripts/windows/README.md` and file presence, not by running PowerShell.
- `usage-check.py` is described from its docstring only.
- Running `ci-failures.sh --help`, `pr-status.sh --help` and `corpus-progress-check.sh` invoked `gh` read-only (those scripts ignore `--help`); worktree stayed clean.

## Follow-ups

- `scripts/windows/README.md` (out of scope) lacks a `pr-conflicts.ps1` row.
- `mct-cli --help` and `mct-mcp-server --help` lack `--version` on the server side; COMMANDS.md says nothing about it, fine.
- The scripts ignoring `--help` (`ci-failures.sh`, `pr-status.sh`, `corpus-progress-check.sh`, `branch-worktree.sh`, `pr-body.sh`) could handle it.
