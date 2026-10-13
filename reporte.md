# Documentation refresh report

Date: 2026-10-13. Base commit: `2641f50` (branch `chore/agent-sandbox-policy`).
Six agents each audited a disjoint set of Markdown files against the real code, in their own branch and worktree. Nothing was pushed and no PR was opened. Code was explored through the project's own MCP tools; `cargo test` and the benchmarks were not re-run by any agent.

Each branch's own `reporte.md` (except `docs-research`, whose agent was refused permission to write one) holds the detailed findings table.

## Rebase onto main (2026-10-13)

The audit ran against base `2641f50`; `origin/main` was 132 commits ahead (new `call` subcommand, TOON default, `mct-languages` crate, Apache-only licensing). The doc commits were cherry-picked onto `docs/refresh-20261013` (from `origin/main`). Conflicts were resolved by keeping main's text where it already fixed the same drift, and adding the audit's change only where main lacked it:

- `CHANGELOG.md`: kept main's Unreleased entries; added only the website landing-page bullet.
- `ISSUES_PENDING.md`: kept main's closed item 9; added the "known limitations" section after it.
- `internal/checklist.md`: kept main's `LICENSE` line.
- `CONTRIBUTING.md`: kept main's registration steps (via `mct-languages`) and CI text; the audit's older "two `build_registry` functions" wording was dropped.
- `COMMANDS.md`: kept main's `format` note and compatibility paragraph; added the `batch` paragraph.

Findings below that mention two `build_registry` functions or a "no-conflict" merge describe the audit base, not main. Facts were not re-verified against main's code after the rebase.

## Branches

| Branch | Worktree (sibling of the repo) | Files | Commits |
|---|---|---|---|
| `claude/docs-roadmap` | `mini-consumes-tokens-docs-roadmap` | `ROADMAP.md`, `ISSUES_PENDING.md` | 19966e0, cffcd69, df7fcdd, 647b4db |
| `claude/docs-commands` | `mini-consumes-tokens-docs-commands` | `COMMANDS.md`, `scripts/README.md` | 7b4230d, 1350ddb, f0568ab |
| `claude/docs-readme` | `mini-consumes-tokens-docs-readme` | `internal/checklist.md` (`README.md` unchanged) | f767a95, 04cbc9d |
| `claude/docs-architecture` | `mini-consumes-tokens-docs-architecture` | `ARCHITECTURE.md`, `CONTRIBUTING.md` | 11bf1ad, 85c189a, ee12e80 |
| `claude/docs-changelog` | `mini-consumes-tokens-docs-changelog` | `CHANGELOG.md`, `RELEASING.md`, `benchmarks/agent-benchmark.md` | d2862d4, 5f02209, 55623e6, c8bf91e |
| `claude/docs-research` | `mini-consumes-tokens-docs-research` | `research.md`, `internal/corpus-progress.md`, `internal/audit-integration-review.md`, `website/README.md` | d8ef5f8, a07f5df, 094e8b7, a5614ba |

## 1. Roadmap and pending issues (`docs-roadmap`)

- `ROADMAP.md`: added a "Status as of 2026-10-13" table for all 12 candidates; original text kept as history; Spanish headings translated.
  - Shipped: `find_dead_code`, semantic search (`hybrid_search`), Kotlin, `find_types` (covered by `list_symbols` `kind`).
  - Partial: HTML/CSS crates exist but no JSX/TSX structure; relation pagination done, no parallel parsing.
  - Open: Swift, Ruby, Ruby/PHP/Swift secret patterns, `find_implementations`, `find_tests`, token-savings tool, LSP.
  - The old 6-8 tool budget no longer applies (18 tools).
- `ISSUES_PENDING.md`: all five `#[ignore]` tests still exist, so items 4-8 stay open; added a section for limitations tracked without an ignored test (#133, #82, #87, #115, #116, #51, #22) and a reference to #135.
- Unverified: GitHub Release v0.2.0 (tag exists), live issue states (taken from the CHANGELOG), Rust/Python fixtures and large-repo benchmarks.
- Follow-up: file issues for the three bugs not yet filed (items 6-8); implement the secret exclusion patterns.

## 2. Commands (`docs-commands`)

- `COMMANDS.md`: added `mct-cli probe` and `mct-mcp-server --rebuild-embeddings`; documented `search_symbols`, `hybrid_search`, `build_context_pack`, `batch`; corrected `limit`/`offset` wording (search tools default 10, clamp 100; `hybrid_search` uses `top_k`; `list_symbols`/`find_symbol` have no `offset`); added missing parameters for `get_indexing_status` and `get_project_overview`; extended the `format: toon` list; fixed a README anchor.
- `scripts/README.md`: Windows section now lists all 15 `.ps1` twins (`branches-status`, `publish-branch`, `release` are Unix-only); added `mcp-smoke.sh --bin/--root`; documented `pr-conflicts.py` and `usage-check.py`; renamed "Installer regression" to "Regression tests".
- Matched reality, left alone: nine `mct-cli` help blocks and all 18 Unix scripts.
- Incident: running `--help` on `ci-failures.sh`, `pr-status.sh` and `corpus-progress-check.sh` executed them (they ignore `--help`); only read-only `gh` calls were made and the worktree stayed clean.
- Unverified: PowerShell behavior, `get_file_tree` pruning claim, `depth`/`format` schemas of the graph tools, `hybrid_search` lexical fallback (from the tool description), `usage-check.py` (docstring only).
- Follow-up: `scripts/windows/README.md` lacks a `pr-conflicts.ps1` row; `ci-failures.sh`, `pr-status.sh`, `corpus-progress-check.sh`, `branch-worktree.sh` and `pr-body.sh` should handle `--help`.

## 3. README and checklist (`docs-readme`)

- `README.md` matched reality and is unchanged: 17 languages registered in both `build_registry` functions, 18 MCP tools, 0.2.0, Apache-2.0, Rust 1.96.0, links, env vars, CLI subcommands.
- `internal/checklist.md`: status note with verified state, title updated to `mini-consumes-tokens`, stale counts and names fixed (languages, tools, crates, license file, dropped crates.io install path), duplicated items removed, the "untracked skills-lock.json" finding marked stale. Historical `MANUAL.md` mentions left on purpose.
- Unverified: test count (204), CI results, README benchmark percentages, existence of 0.2.0 prebuilt archives, `skills-lock.json` contents.
- Follow-up: `checklist.md` is mostly a session log with stale numbers; consider archiving it or moving its history to the CHANGELOG.

## 4. Architecture and contributing (`docs-architecture`)

- `ARCHITECTURE.md`: tool count 9 → 18 (matches `KNOWN_TOOL_NAMES`); removed the claim that Lua is not shipped; dropped the "261 tests" figure; added the `mct-core` registry role, the `mct-tree-sitter` helper crate and the 10 migrations in `schema.rs`.
- `CONTRIBUTING.md`: language crates also depend on `mct-tree-sitter`; new crate needs workspace `members` and `[workspace.dependencies]` entries; registration lists both `build_registry` functions, both `Cargo.toml` dependencies and the fuzz CI matrix entry (fuzz harness is required); "11 tools" → 18; CI has four jobs (added `fmt`), clippy denies `unwrap_used`/`expect_used`/`panic`, fuzz matrix is 17 language crates plus `mct-index`.
- Verified, no change: `LanguageParser` methods, duplicate-extension panic in `register`, `ParseError::Syntax`, TTC tests, fuzz layout.
- Unverified: real test count, secret-skipping/fingerprinting/security-scan claims, "~68% smaller" TTC figure (kept, labelled historical), `docs/06-templates/lang-crate-base.md` (not reviewed).
- Out of scope, needs a decision: `rules.md` still says to register in "exactly two places", its `mct-mcp-server` list omits `discover_tool_categories` and `get_tool_schema`, and its `mct-cli` subcommand list is incomplete. `rules.md` was not touched because it has your uncommitted changes.

## 5. Changelog, releasing, benchmarks (`docs-changelog`)

- `CHANGELOG.md`: `[Unreleased]` already covered `release.sh`, `mct-cli update` and the 3D foundation; added the missing `website/` entry (landing page and docs site, 20+ commits since v0.2.0).
- `RELEASING.md`: steps match `release.sh`; the pre-flight now lists the version spots `release.sh bump` does not touch (README sentence, `--version` example, `site/index.html` eyebrow, final `git grep`).
- `benchmarks/agent-benchmark.md`: the `main.rs:731/732` call sites are valid only at commit `c06ab5c`; noted as such. All referenced paths and the pinned commit exist.
- Unverified: no benchmark figure was re-run; cargo commands cited by the benchmarks were not executed; the website entry was not checked against a build.
- Needs a decision: `site/index.html:88` still says `Field reference · v0.1.0 workspace` while the workspace is 0.2.0. Consider extending `release.sh bump` to cover the manual spots.

## 6. Research notes, corpus, audit, website (`docs-research`)

This agent's own `reporte.md` was refused by the Write tool; this section is built from its returned report.

- `research.md`: new "Status update — 2026-10-13" section above §1 with a finding/status/evidence table; original text untouched.
  - Shipped: `.claude/` excluded from indexing (A4), overview default and byte budget (A2/A5), one-line dependency summary in status (A3), `path`/`language` scope filters (B1/B4), `to_symbol_id` removed (B2), `find_symbol` `match` modes backed by FTS (B3), directory heuristic (B5), test-name heuristic (B6), `is_top_level` (B7), walk pruning (C1), `.mctignore` with reload and `!` re-includes plus opt-in `.gitignore` import (C2), overview fan-in (C4), Markdown module symbols and frontmatter relations (D1-D3).
  - Partly shipped: watcher (C8) still watches the whole root but reindexes only changed paths.
  - Still open: `tools/list` byte-ceiling test (A1), connection pragmas beyond WAL and foreign keys (C7).
  - Not implemented: `get_note_context` (D4).
- `internal/corpus-progress.md`: re-counted files and lines for all 17 languages (all rows match); noted #74 is CLOSED and #79, #82, #115, #116 are OPEN.
- `internal/audit-integration-review.md`: mapped dead review hashes to their merged commits (PR #91: f4bd23c, 11df400, 0e5d352, 991299b; report f60d9d2); findings E1 and E2 remain open.
- `website/README.md`: documented `scripts/serve.sh` (macOS-only, calls `open`) and `npm start`; all other commands exist in `package.json`.
- Unverified: symbol/relation counts in the corpus table (need `cargo test`), `canonicalize()` after exclusion (C1), `prepare_cached` use (C3), replacement for `idx_relations_kind` (C6), current `tools/list` size, RS2 and L1, any website execution.
- Follow-up: `rules.md` still mentions #74 in its command list although it is closed; schedule A1, C7 and C8 work; `ROADMAP.md`/`ISSUES_PENDING.md` were not cross-checked by this agent.

## Cross-cutting items for you to decide

1. `rules.md` drift (registration points, tool list, CLI subcommands, #74) — not edited because of your uncommitted changes.
2. `site/index.html` eyebrow still `v0.1.0`.
3. Whether to archive `internal/checklist.md`.
4. Issues to file: ROADMAP/ISSUES_PENDING items 6-8; `--help` handling in five scripts; missing `pr-conflicts.ps1` row.
5. Nothing was measured: benchmark figures, test counts and CI results across all docs remain as previously written.

## Integration

The six branches touch disjoint files and should merge without conflicts. Review each diff with `git diff 2641f50..claude/docs-<name>`; merging or opening PRs is left to you.
