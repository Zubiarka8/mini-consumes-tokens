# Report: ROADMAP.md and ISSUES_PENDING.md refresh

Date: 2026-10-13. Branch `claude/docs-roadmap`, base `2641f50`.

## What was researched

- `rules.md` (Dogfooding, Commands), `ROADMAP.md`, `ISSUES_PENDING.md`, `CHANGELOG.md` (Unreleased and 0.2.0), `docs/05-backlog/roadmap.md`, `git log`, `git tag`.
- Source via the project MCP tools (`search_symbols`, `find_symbol`) for `DEFAULT_EXCLUDE_PATTERNS` and `for_extension`; `git grep` over the worktree for `#[ignore`, `KNOWN_PENDING_LANGUAGES`, `rayon`, `find_implementations`, `find_types`, `find_tests`, CLI subcommands.
- No `.claude/skills` directory exists in the worktree, so no local skill was used.

## Findings

| File | Claim | Evidence | Action |
|---|---|---|---|
| ROADMAP | Kotlin pending | `crates/mct-lang-kotlin`; `KNOWN_PENDING_LANGUAGES` lists only swift, rb | Marked shipped; Swift/Ruby open |
| ROADMAP | `find_types` missing | `list_symbols` has `kind` | Marked shipped differently |
| ROADMAP | `find_dead_code` candidate | MCP tool and `mct-cli dead-code` in CHANGELOG 0.2.0 | Marked shipped |
| ROADMAP | `semantic_search` deferred | `hybrid_search`, local embeddings, `--features semantic` | Marked shipped |
| ROADMAP | Secret patterns for Ruby/PHP/Swift | Absent from `exclude.rs` | Confirmed open |
| ROADMAP | `find_implementations`, `find_tests`, token-savings tool, LSP | Not found in code | Confirmed open |
| ROADMAP | 6-8 tool budget | 18 tools in 0.2.0 | Noted as no longer applying |
| ROADMAP | Perf hypotheses | No `rayon`; `relation_scale.rs` exists | Marked partly done |
| ROADMAP | Spanish headings | Repo content must be English | Renamed to "Candidates evaluated", "Suggested order" |
| ISSUES_PENDING | Items 4-8 open | All five `#[ignore]` tests still present in crates | No change |
| ISSUES_PENDING | Alias wikilinks | CHANGELOG cites #135 | Added reference |
| ISSUES_PENDING | Missing limitations | CHANGELOG lists #133, #82, #87, #115, #116, #51, #22 | Added a section |

## Changes

- `ROADMAP.md`: new banner, "Status as of 2026-10-13" table (12 candidates) and a list of shipped work not in the original; headings translated. Original candidate text left intact as history.
- `ISSUES_PENDING.md`: #135 reference; section "Known limitations tracked without an ignored test".

## Not verified

- Whether the GitHub Release for v0.2.0 is published (tag exists).
- State of GitHub issues (#133, #82, #87, #115, #116, #51, #22, #135); taken from CHANGELOG only.
- Rust/Python integration fixtures and benchmarks (candidate 12).
- Whether a large synthetic-repository benchmark exists outside the searched paths.
- The MCP index may reflect the main checkout rather than this worktree.
- Line numbers in ISSUES_PENDING item 5 (227 vs 236) were not rerun.

## Suggested follow-ups

- File GitHub issues for Java, Kotlin and Rust bugs (items 6-8, "not filed yet") and link them.
- Implement the secret exclusion patterns (small, independent).
- Decide whether to retire the ROADMAP candidate text in favour of `docs/05-backlog/roadmap.md`.
