# Backlog

## Deferred from `mct-lang-md` Phase 2
See spec `docs/design/2026-09-17-obsidian-docs-vault-and-md-parser-design.md`.
- Anchor-aware resolution of `[[Page#Heading]]` against the target file's real heading symbol.
- Tags/links inside list items, blockquotes, tables.
- Code-span exclusion for `#tag`/`[[...]]` (needs an inline-grammar reparse).
- Exact-span relation locations instead of block-granular.

## Token-efficiency improvements
- **BFS query cache in SQLite.** Cache results of expensive multi-hop traversals (`find_references`/`find_calls`/`find_callers`/`impact_analysis` with `depth` > 1) keyed by `(symbol_id, tool, depth, offset)`, invalidated whenever `reindex` runs. Avoids recomputing the same graph walk if an agent repeats a similar query within a session. **Touches the SQLite schema** (new cache table) — per `CLAUDE.md`'s cross-cutting rule, this needs an issue opened first, not a drive-by PR.
- **Agent-maintained Obsidian session note.** At the close of a task, the agent appends a short note (decisions made, why, what's left) to a per-session or per-topic page in this vault, so the next session starts by reading one note instead of reconstructing context from `git log`/conversation history. Doesn't touch `crates/` or the MCP tool surface — pure docs workflow, no cross-cutting constraint.

## Web / 3D portfolio support (by priority)
P0 must land first.
- [x] **P0 Foundation design:** ADR-003 (asset ingestion, kinds, asset path resolution, embedded languages, MCP surface, shared fixture, migration) + issue draft.
- [x] **P0b Foundation implementation:** core changes from [ADR-003](../01-architecture/adrs/003-web-3d-indexing.md):
  - D1: `mct-index` asset pre-pass (`glb gltf png jpg jpeg webp ktx2 hdr exr` → `files` row with `language = "asset"` + one `asset` symbol). Images stat-only. glTF JSON ≤ 8 MiB. `.glb` JSON-chunk reader that never reads BIN. Bad input → `syntax_error` issue. Fuzz target.
  - D2: `SymbolKind` `asset`, `model_node`, `material`, `animation`, `finding`, updated in every exhaustive match. No new `RelationKind`.
  - D3/D4: `mct_core::resolve_reference_path`; `mct_tree_sitter::mask`.
  - D5: optional `get_indexing_status.dependency`; `kind` descriptions list the new kinds; no source lines for `asset` definitions in `build_context_pack`.
  - D6/D7: `portfolio-3d-app` fixture base + second `mct-eval` suite. No schema migration and no `reindex --force`.
- [ ] **`.astro` files:** split frontmatter from template; link components, data and styles.
- [ ] **JSX/TSX elements as symbols:** index used components, props, events and component-to-component relations (today only the surrounding code is analyzed).
- [ ] **Code → 3D asset relations:** link `useGLTF`, `useTexture`, `<Model src>` and similar references to `.glb`, textures, images and HDR environments.
- [ ] **`.glb`/`.gltf` inspection:** summarize scenes, nodes, meshes, materials and animations to see what each experience uses.
- [ ] **Web project config:** index `package.json` dependencies and Astro, Vite, TypeScript and Tailwind config files.
- [ ] **Feature context bundle:** one short answer with the 3D component, its assets, styles, dependencies and tests before editing a scene.
- [ ] **Style and accessibility map:** link CSS/Tailwind classes to components; flag scene controls that need keyboard, touch or reduced-motion alternatives.

## Prioritized improvement candidates
Ordered in [ADR-025](../01-architecture/adrs/025-prioritized-improvement-candidates.md) (status: Proposed). Work in this order; items that change the SQLite schema or the MCP tool surface need an issue first.
- [ ] **1. Secret exclusions for Ruby, PHP and Swift:** `wp-config.php`, `config/database.php`, `config/master.key`, `config/credentials.yml.enc`, `*.xcconfig`, `GoogleService-Info.plist` (`crates/mct-index/src/exclude.rs`). Highest priority.
- [ ] **2. Relation resolution state in tool output:** confirm every relation tool shows `resolved`, `ambiguous` or `unresolved`, and document the meaning.
- [ ] **3. `mct-cli report`:** one architecture report combining most-used symbols, import cycles, duplicates and dead-code candidates. Reuse the existing modules; avoid duplicating the web overview.
- [ ] **4. Instruction-file installer:** idempotent, delimited block for `AGENTS.md` and `CLAUDE.md` pointing to `rules.md`. Must never delete user content.
- [ ] **5. Query log and token-savings tracking:** metadata only, never source text. Covers the observability item above.
- [ ] **6. `NOTE`, `WHY` and `TODO` annotations** queryable from `find_symbol` and `build_context_pack`. Schema change; issue first.
- [ ] **7. Module community detection.** Deferred; needs a deterministic algorithm.
- [ ] **8. Exclude generated build output by default:** `website/out`, `website/.next`, generated CSS in `crates/mct-cli/web-ui`. Removes ERR-023 and ERR-024 noise (`exclude.rs`).
- [ ] **9. Name the files omitted from duplicate and dead-code results,** with a reason for each. Counts already exist.
- [ ] **10. Suggest the correct parameter name** when a tool call uses an unknown one (for example `name` instead of `symbol`), ERR-014.
- [ ] **11. Show the indexed root and file count** in responses, and name the expected root when a path is not found, ERR-016.
- [ ] **12. Detect concurrent writers** to one index and keep build artifacts separate per worktree, ERR-018. Schema-free; detection and documentation.
- [ ] **13. Polling fallback** when the native file watcher delivers no events, ERR-009 (macOS FSEvents).
- [ ] **14. Separate expected malformed fixtures** from real parse failures in indexing status, ERR-003 (16 of 20 diagnostics are intentional).
- [ ] **15. Tailwind v4 CSS directives** (`@source`, `@theme`, nesting, `@import` forms), issues #87, #115, #116 and ERR-023. May depend on upstream grammar fixes.
- [ ] **16. Valid syntax rejected by the parsers:** Bash `coproc` (#79) and C++ constructs (#82). One small fixture per case.
- [ ] **17. `find_tests(symbol?)`:** tests that cover a symbol. Adds a tool; issue first.
- [ ] **18. Inheritance tree** for a class or interface: parents, children and implementations, from the existing `Extends` and `Implements` relations.
- [ ] **19. Call-flow diagram** from one symbol, to N hops, as Mermaid or DOT.
- [ ] **20. Hotspots:** large functions with many callers, ranked by size and caller count. Uses `end_line` and existing caller counts.
- [ ] **21. Public surface of a module:** the symbols it exposes. Needs visibility captured per parser; not stored today.
- [ ] **22. Hidden coupling:** files that change together in git history without a direct dependency. Needs a bounded history walk.
- [ ] **23. Change impact from a diff or branch:** affected symbols, callers and tests for the changed files. Adds a tool; issue first.
- [ ] **24. Path between two symbols:** the chain of relations connecting them. Adds a tool; issue first.
- [ ] **25. Compare two indexes** (two branches or two roots): added, removed and changed-signature symbols.
- [ ] **26. Export the full index** to JSON or SQLite for external tools and dashboards.
- [ ] **27. Documentation coverage:** public functions and types without a doc comment. Depends on 21 and on storing doc comments.
- [ ] **28. Exact name resolution** through SCIP ingestion or an LSP. Largest precision gap; changes the core symbol model. Design record first.
- [ ] **29. Faster indexing of large repositories** through parallel parsing. A cold 9,288-file copy took 57 to 136 seconds (ERR-022).
- [ ] **30. Semantic search in the prebuilt release binaries.** Today the `semantic` feature is built only from source.
- [ ] **31. Wider distribution:** publish to crates.io (blocked by the vendored `tree-sitter-md` patch) and add package-manager formulas. One-command installers already exist.
- [ ] **32. Public, reproducible benchmark** against other tools on a public repository. Publish the losing cases too.
- [ ] **33. Relations across roots and monorepos** for `--roots`. Each root is independent today.
- [ ] **34. JSX and TSX components as symbols:** used components, props and events (issue #51).
- [ ] **35. `mct-cli clean`:** show the size of `.mct-index/` and reclaim it. Dry run by default; `--yes` runs `VACUUM` and truncates the WAL; `--obsidian` also deletes the vault; `--all` deletes `.mct-index/` (rebuilt on next use). Never deletes `review-decisions.toon` or `finding-numbers.tsv` unless `--include-reviews` is passed. Takes the maintainer lock first and stops if a server or `web` holds it. Never touches anything outside `.mct-index/`. Expose the same function as an MCP tool `clean_index` (adds a tool; issue first) and as a "Storage" panel with a "Compact index" button in `mct-cli web`. Expected gain is tens of MB per project: the index is bounded by project size, not by usage.
- [ ] **36. `scripts/unix/clean-merged-worktrees.sh`:** run `cargo clean` in every worktree whose branch is already merged into `main`, skipping worktrees with running processes. Development-only; the gigabytes on a contributor machine come from per-worktree `target/` folders (10–22 GB each), not from mct.

## Open project decisions
- HTML template-engine handling (Django/Jinja2 embedded in `.html`) — undecided, see `internal/checklist.md`.
- PHP-specific secret patterns (`wp-config.php`, `config/database.php`) — named, not implemented.

## Related
[[00-index]]

#backlog
