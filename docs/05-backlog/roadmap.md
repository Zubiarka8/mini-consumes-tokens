# Backlog

## Deferred from `mct-lang-md` Phase 2
See spec `docs/superpowers/specs/2026-09-17-obsidian-docs-vault-and-md-parser-design.md`.
- Anchor-aware resolution of `[[Page#Heading]]` against the target file's real heading symbol.
- Tags/links inside list items, blockquotes, tables.
- Code-span exclusion for `#tag`/`[[...]]` (needs an inline-grammar reparse).
- Exact-span relation locations instead of block-granular.

## Token-efficiency improvements
- **BFS query cache in SQLite.** Cache results of expensive multi-hop traversals (`find_references`/`find_calls`/`find_callers`/`impact_analysis` with `depth` > 1) keyed by `(symbol_id, tool, depth, offset)`, invalidated whenever `reindex` runs. Avoids recomputing the same graph walk if an agent repeats a similar query within a session. **Touches the SQLite schema** (new cache table) — per `CLAUDE.md`'s cross-cutting rule, this needs an issue opened first, not a drive-by PR.
- **Agent-maintained Obsidian session note.** At the close of a task, the agent appends a short note (decisions made, why, what's left) to a per-session or per-topic page in this vault, so the next session starts by reading one note instead of reconstructing context from `git log`/conversation history. Doesn't touch `crates/` or the MCP tool surface — pure docs workflow, no cross-cutting constraint.

## Web / 3D portfolio support (by priority)
Prompts per item: [web-3d-prompts](web-3d-prompts.md). P0 must land first.
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

## Open project decisions
- HTML template-engine handling (Django/Jinja2 embedded in `.html`) — undecided, see `internal/checklist.md`.
- PHP-specific secret patterns (`wp-config.php`, `config/database.php`) — named, not implemented.

## Related
[[00-index]]

#backlog
