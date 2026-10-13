# Report: docs-readme (README.md and internal/checklist.md audit)

Base 2641f50, branch claude/docs-readme.

## What was researched
- README.md read in full; internal/checklist.md read in full.
- Registered languages: `mct-lang-*` dependencies of `crates/mct-mcp-server/Cargo.toml` and `crates/mct-cli/Cargo.toml` (both list the same 17), plus the 17 `crates/mct-lang-*` directories.
- MCP tools: `discover_tool_categories` on the running server (18 tools: 4 discovery, 3 lookup, 5 relations, 3 maintenance, 3 meta).
- Version, license and toolchain: root `Cargo.toml` (0.2.0, Apache-2.0), `ci.yml` (Rust 1.96.0).
- Linked files checked with `ls`: LICENSE, THIRD_PARTY_NOTICES.md, COMMANDS.md, ARCHITECTURE.md, CONTRIBUTING.md, CHANGELOG.md (anchor `Upgrading from 0.1.0` present), RELEASING.md, install.sh, install.ps1, `.codex/config.example.toml`, `docs/03-performance/query-cache.md`.
- Env vars and feature claims: `MCT_MAX_FILE_BYTES` (read_limits.rs), `MCT_CACHE*`, `MCT_SEMANTIC_CACHE`, `MCT_EMBEDDING_MODEL`, `bge-small-en-v1.5` (query-cache.md, THIRD_PARTY_NOTICES.md), `semantic` feature (mct-mcp-server Cargo.toml), `MCT_INSTALL_DIR` (install.ps1), CLI subcommands (COMMANDS.md).

## Findings
| File | Claim | Evidence | Action |
|---|---|---|---|
| README.md | 17 languages, list | 17 crates registered in both binaries; list matches | none |
| README.md | 18 MCP tools | discover_tool_categories lists 18 | none |
| README.md | version 0.2.0, Rust 1.96.0, Apache-2.0 | Cargo.toml, ci.yml | none |
| README.md | links, `.codex/config.example.toml`, query-cache.md | files exist | none |
| README.md | semantic feature, toon, env vars, CLI commands | present in Cargo.toml, docs, COMMANDS.md | none |
| checklist.md | title `codebase-context-mcp` | renamed | title fixed, status note added |
| checklist.md | "7 tools (6-8 limit)" | 18 tools now | marked superseded |
| checklist.md | "eight languages" (open decision 1) | 17 | fixed |
| checklist.md | crate list in "General deliverable" | missing kotlin, md, tree-sitter, corpus, eval (24 crates) | added |
| checklist.md | `LICENSE-MIT` | file is `LICENSE` (Apache-2.0) | fixed |
| checklist.md | README documents crates.io install | README documents two paths; crates.io blocked (RELEASING.md §4) | fixed |
| checklist.md | language table broken by blank lines before Bash and PHP rows | rendering | blank lines removed |
| checklist.md | duplicated items (5.1-5.7 cleanup, Bash/PowerShell) | duplicate text | duplicates removed |
| checklist.md | "untracked skills-lock.json" | `git ls-files` shows it tracked | marked stale |
| checklist.md | references to MANUAL.md/MANUAL.html | files do not exist | covered by status note; historical entries left |

## Changes
- README.md: none (no drift found).
- internal/checklist.md: items above.
- reporte.md: this file.

## Unverified
- Test counts (204) and CI run results in checklist.md were not re-run.
- Benchmark percentages in README (70-98%, 97.8%, 37-42%, 85%) were not re-measured.
- skills-lock.json contents were not inspected.
- Whether prebuilt release archives exist for 0.2.0 on GitHub (network not checked).
- Exact CLI flags were checked against COMMANDS.md only, not the binary.

## Follow-ups
- checklist.md is mostly a session log with stale historical numbers; consider archiving it or moving history into CHANGELOG.
- Historical entries still mention MANUAL.md and old project names on purpose.
