---
name: reduce-duplication
description: Find and safely reduce genuine code duplication in mini-consumes-tokens while preserving crate boundaries, behavior, and project workflow. Use when repeated logic appears to need coordinated changes.
license: CC0-1.0
metadata:
  version: 1.0.1
  kind: prompt
  category: refactoring
  source: https://hermes-ide.com/prompts/reduce-duplication
  catalog: 2026.1014.1
---

# Reduce code duplication in mini-consumes-tokens

## Inputs

- **Scope** (required): a directory, crate, module, files, or a description of suspected duplication.

Use the scope provided by the user. If it is missing, ask for it once before exploring or changing code. Do not expand the scope without explaining why and getting direction when the expansion changes the task.

## Project context

This repository is a Rust workspace organized around strict crate boundaries:

- `mct-core` owns parser and registry contracts and shared symbol models. It must not know about SQLite, tree-sitter, or MCP.
- `mct-index` owns SQLite schema, migrations, indexing orchestration, and queries. It must not depend on language grammar details.
- Each `mct-lang-*` crate implements one language parser. It must not know about SQLite or MCP, and parsing untrusted source must not panic.
- `mct-mcp-server` exposes MCP tools and composes the index and language registry.
- `mct-cli` owns command-line workflows; `mct-eval` owns development-only quality evaluation.

Treat `rules.md` as authoritative. In particular, changing `LanguageParser`, the SQLite schema, or a published MCP tool signature is a cross-cutting change that requires an issue first. Do not use a shared helper to blur crate ownership or create a dependency in the wrong direction.

## Workflow

1. **Preserve the starting state.** Inspect the current branch and working tree. Do not overwrite, revert, stage, or reformat unrelated work. Keep changes limited to the requested scope.
2. **Explore with project tools.** For source-code discovery, follow the dogfooding rules in `rules.md`: use this project's MCP tools and `mct-cli`, not broad shell searches or file dumps. Start with `get_project_overview` or `get_file_tree` when needed, then use `list_symbols`, `get_file_skeleton`, `build_context_pack`, and relation tools to inspect candidates. Use `batch` for known independent lookups. Read files only as needed for editing or when the project rules allow it.
3. **Check index reliability.** Use `get_indexing_status` before relying on indexed results. If the index is stale or its freshness cannot be established without writes, report that limitation. Do not reindex or mutate the index unless the user authorizes it.
4. **Inventory candidates before editing.** Look for repeated blocks, near-identical functions, parallel match logic, and repeated validation or formatting. Record each candidate's locations, behavior, callers, and apparent ownership.
5. **Classify the repetition.** Call it genuine duplication only when the copies encode the same rule and are expected to change together. Use code behavior, requirements, call relationships, and available history as evidence; history is useful but not mandatory or conclusive. Treat similar-looking code as coincidental when its owners or reasons to change differ. State uncertainty instead of guessing.
6. **Choose a small, coherent extraction.** There is no fixed copy-count threshold. Extract only when a well-named shared function makes the common rule clearer and reduces coordinated maintenance. Keep parameters few; do not add boolean mode flags, generic frameworks, class hierarchies, or cross-crate dependencies just to eliminate similar text. Respect public API and architecture constraints.
7. **Handle drift explicitly.** Compare edge cases, errors, defaults, ordering, and side effects before merging. If copies differ and project requirements do not establish which behavior is correct, leave them separate and report the decision needed. Do not silently change behavior.
8. **Make changes incrementally.** Refactor one logical group at a time. Keep behavior unchanged, preserve existing public contracts, and update focused documentation only when needed to explain the shared rule.
9. **Use the project scripts where they fit.** The scripts in `scripts/` are focused helpers, not a general duplicate-code detector. Do not run them just to discover candidates. When the user asks for verification, prefer the documented wrapper and its narrowest useful option:
   - `scripts/unix/check.sh -p <crate> --only fmt`, `--only test`, or `--only clippy` for a scoped Rust change on macOS/Linux; use the matching `scripts/windows/check.ps1` options on Windows.
   - `scripts/unix/check.sh` (or `scripts/windows/check.ps1`) for the full project check only when the requested verification calls for it.
   - `scripts/unix/parse-probe.sh <files...>` (or `scripts/windows/parse-probe.ps1`) only when the refactor concerns parser behavior and a direct, non-indexed parse probe helps investigate a concrete input. A probe does not replace the crate's tests.
   - `scripts/unix/corpus-report.sh <lang>` (or `scripts/windows/corpus-report.ps1`) only when a parser change affects that language's long-fixture corpus and the user has asked for the corresponding check. Do not use `--bless` or `--update-progress` as part of a behavior-preserving refactor without explicit authorization.
   - `scripts/unix/new-tool-check.sh` and `scripts/unix/new-language-check.sh` (and their Windows counterparts) are for adding a tool or language, not routine deduplication. If the proposed extraction starts to require either workflow, stop and reassess scope and architecture.

   Follow `rules.md` and the user's verification request if they differ from these examples. Do not add or run tests, builds, benchmarks, or other checks unless the user asks for verification. Report exact commands and outcomes; never claim an unrun check passed.
10. **Review the resulting diff.** Confirm that only the intended files changed, no unrelated work was disturbed, the abstraction stays within the correct crate boundary, and the moved code preserves each caller's behavior.

## Constraints

- Do not make behavior changes as part of a duplication-only refactor.
- Do not merge coincidental duplication; briefly explain why it should stay separate.
- Do not modify public MCP signatures, `LanguageParser`, or the SQLite schema as a drive-by refactor. Follow the issue-first rule in `rules.md`.
- Never introduce `unwrap()` or `panic!` on paths that process repository input. Parsers handle arbitrary third-party source.
- Avoid unrelated cleanup, formatting churn, dependency changes, and scope expansion.
- Write all repository content in English, including identifiers, comments, and documentation.
- Do not alter the local index or other generated state during exploration.

## Report

### Candidate duplication

For each candidate, report locations (`path:line`), classification (genuine or coincidental), evidence, the rule each copy implements, behavior differences, ownership/change reasons, and whether it was changed or left alone. Explain why any similar-looking code was intentionally kept separate. Mark evidence that could not be confirmed as unknown; do not fill gaps with assumptions.

### Changes

Summarize the shared rule extracted, the files changed, and why the abstraction belongs at that location. Mention any relevant architecture or API boundary.

### Verification and limits

List only checks actually requested and run, with exact commands and results. If no checks were requested, say they were not run. Include stale-index limitations, unresolved behavior drift, unavailable evidence, decisions still needed, and any scope questions.
