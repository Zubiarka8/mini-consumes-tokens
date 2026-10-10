# ADR-003: Web / 3D portfolio indexing foundation

## Status
Accepted (2026-10-10). Issue: pending (draft below). Covers the seven items under "Web / 3D portfolio support" in the [roadmap](../../05-backlog/roadmap.md#web--3d-portfolio-support-by-priority); prompts in [web-3d-prompts](../../05-backlog/web-3d-prompts.md). Builds on [ADR-002](002-qualified-relation-resolution.md) (`RelationTarget` resolution).

## Context
- `SourceFile.contents` is `String`. `index_file` drops a non-UTF-8 file before any parser, and a file with no registered extension gets no `files` row, so a `.glb`, `.png` or `.hdr` has no node for a relation to resolve to.
- `SymbolKind` has 14 values and `RelationKind` has 5. Both are stored as TEXT (`symbol_kind_str`/`relation_kind_str`) and read back as plain strings. `kind_heading` falls back to `"<kind>s"`, and the `kind` param of `list_symbols` is a free string (`find_symbol` and `search_symbols` have none). A new kind needs no SQL migration, but MCP clients can see it.
- Precedents already in the code: `manifests.rs` matches file names in `mct-index` and parses `package.json`/`Cargo.toml` with serde/toml, outside `LanguageParser`. HTML `class` → CSS `Rule` sets `RelationTarget.language`. Every source read is capped by `read_repository_file` (16 MiB, `MCT_MAX_FILE_BYTES`). `index_file` stores only canonical paths inside the root.
- Lang crates depend on sibling lang crates only as dev-dependencies, for integration tests (`mct-lang-html` → `mct-lang-css`, `mct-lang-xaml` → `mct-lang-csharp`).
- The schema has 10 migrations. A newer migration count stops an older binary at startup (`CONNECTION_CLOSED`, see [rules.md](../../../rules.md#dogfooding-how-to-explore-this-repositorys-source-code)).

## Decision

### D1 Asset ingestion
| Option | Verdict |
|---|---|
| (a) `mct-index` pre-pass, like `manifests.rs`, plus an `assets` table | Pre-pass **chosen**. The new table is rejected because an `assets` row is not a symbol, so `relation_candidates` could not resolve a relation to it without a view and schema change. |
| (b) `SourceFile.contents` → bytes for every parser | Rejected. It touches all 18 lang crates and the fuzz targets to serve one binary format, and it still reads a 40 MB `.glb` in full. |
| (c) Optional `LanguageParser::parse_bytes` with a default impl | Rejected. It is additive, but it is still a trait change, and the index would still read whole binaries. |

**Decision.** Add a new `crates/mct-index/src/assets.rs`. `index_file` runs it before the registry lookup, as it does for manifests.
- `ASSET_EXTENSIONS` = `glb gltf png jpg jpeg webp ktx2 hdr exr` (one constant; a new format is one edit).
- Each asset file gets one `files` row with `language = "asset"` and one symbol of kind `asset`. The symbol is named after the file name (`robot.glb`), at line 1 and column 1, with no parent. `content_hash` = `asset:<size>:<mtime-secs>`, so an unchanged asset is skipped like source. `Seen::Source` keeps the deletion sweep working.
- Images and HDR/EXR files are only stat'ed, never read. They have no size cap.
- `.gltf` is read in full only when the file is ≤ **8 MiB** (the JSON cap).
- `.glb`: read the 12-byte header and the first 8-byte chunk header. Require magic `glTF`, version 2, chunk type `JSON`, and a chunk length ≤ 8 MiB that also fits the file. Then read exactly that chunk and trim trailing `0x20` padding. The BIN chunk is never read.
- A malformed, truncated, over-cap or non-UTF-8 JSON keeps the `asset` symbol and records a `syntax_error` issue with a detail such as `glb: JSON chunk 9437184 B exceeds 8388608 B cap`. No new `issue_kind` string, so older binaries can read it.
- glTF JSON → symbols (P4) is `crates/mct-index/src/gltf.rs`, serde_json over the text above. Like `package.json`, it is a data format, not a language. Caps: at most **10,000** glTF symbols per file (scenes + nodes + materials + animations), with the rest dropped silently, as the 500-literal cap does. The node hierarchy is walked to `mct_core::MAX_TRAVERSAL_DEPTH` (256) with a visited set, so a child cycle terminates.

*Why:* this follows a precedent that already exists. It changes no trait and no schema. Assets become ordinary resolvable symbols, and a big `.glb` costs one stat plus a bounded prefix read.

### D2 Vocabulary
What each phase stores:

| Phase | Stores | Kinds used |
|---|---|---|
| P1 `.astro` | One `module` per file, named after the file stem (the component name). Frontmatter and `<script>` symbols come from js-ts and `<style>` rules from css. Each template component tag `<Card/>` is a relation from the module to `Card`. | existing + `calls` |
| P2 JSX | Each component tag (capitalized or `a.B`) is a relation from the enclosing function/component to the tag name. Lowercase intrinsics (`div`, `mesh`) are not stored. **Props are not stored**: the relation's location is the opening tag's line, and P7 evaluates the event props it needs at parse time. | `calls` |
| P3 config | Nothing new. Dependencies are already in `dependencies`, and `*.config.{ts,mjs}` are already parsed by js-ts. `tsconfig.json` stays unindexed (no JSON parser), and that is accepted. | none |
| P4 glTF | Scenes and nodes (hierarchy in `parent`; a scene's parent is the asset name), materials, animations. Image URIs become relations to the image asset. Meshes are not stored, because drei code addresses nodes and materials, never meshes. | `model_node`, `material`, `animation`, `references` |
| P5 code → asset | A relation from the loading symbol to the asset's file name, with `RelationTarget { path, language: "asset", kind: Asset }`. | `references` |
| P6 bundle | Nothing; it only queries. | none |
| P7 styles / a11y | `class`/`className`/`clsx`/`cn` literals become relations to `.name` with `language: "css"`, as in HTML. Each heuristic hit is one symbol named after the heuristic, with parent = the enclosing component and location = the element. | `references`, `finding` |

**New `SymbolKind` values (kind string as stored):** `Asset` → `asset`, `ModelNode` → `model_node`, `Material` → `material`, `Animation` → `animation`, `Finding` → `finding`. **No new `RelationKind`.**

*Why:* rendering a component compiles to a call (`jsx(Scene, …)`, `$$renderComponent`). So `calls` holds, and `find_callers`/`find_references`/`impact_analysis` return component usages without changing any tool. Everything else fits `references` + `RelationTarget`. The glTF kinds stay separate because code addresses `nodes.X`, `materials.Y` and clips by kind.

### D3 Asset resolution
Add `mct_core::resolve_reference_path(from_file, spec) -> Option<String>`. It is a pure lexical function with no filesystem access, which parsers can call:
1. Strip `?…` and `#…` (`./tex.png?url` → `./tex.png`).
2. Reject an empty spec, a backslash, a scheme (`http:`, `data:`, `blob:`), `//host`, and an absolute or drive path (`C:/`). The result is `None` and no relation is recorded.
3. A leading `/` is relative to `<app root>/public/`. The app root is the path prefix before the first `src/` segment of `from_file` (`apps/web/src/x.tsx` → `apps/web/`), or else `from_file`'s own directory. This follows Astro/Vite's default `publicDir`. A custom `publicDir` is not read, so such a path stays unresolved and is never misresolved.
4. `./` and `../` are relative to `from_file`'s directory, normalized lexically. A `..` that would climb above the repository root gives `None`. A `..` that stays inside the root is allowed, because `../textures/a.png` is the common case.
5. Bare specifiers (`three`, `@/components/X`) give `None`. tsconfig path aliases are not resolved.

At index time, a `RelationTarget.path` resolves only to a `files.relative_path`, and those are canonical paths inside the root (`index_file` already rejects symlinks that escape it). So a symlink escaping the root and a missing file both show up as `unresolved`, never as a wrong edge. An in-root symlink resolves only through its canonical path, a known limit.
- A dynamic path (template literal with `${}`, a variable, concatenation) records **no relation**, because nothing is proven and there is no name to record.
- A static path to a missing file records a relation that shows as `unresolved`, which flags the broken asset.
- An extensionless module import gets no `path` and stays unqualified, as calls are today.

*Why:* resolution stays inside the parser contract (pure function, no I/O), and the existing root-escape guard does the filesystem half.

### D4 Embedded languages
| Option | Verdict |
|---|---|
| Region splitter that reuses `mct-lang-js-ts`/`css`/`html` | **Chosen** |
| Dedicated `tree-sitter-astro` grammar | Rejected. It is one more grammar dependency, and it still needs injected TS/CSS parsed by the same crates. Vue and Svelte would each need their own grammar. |

**Decision.** Splitting is done by masking, not offset mapping. `mct_tree_sitter::mask(contents, keep: &[Range<usize>]) -> String` replaces every byte outside `keep` with a space, except `\n`/`\r`. Each region (frontmatter `---…---`, `<script>`, `<style>`, the template) is parsed by the sibling parser on a masked copy the same length as the file. Lines and byte columns are then already those of the original file, with nothing to map back. In the template, each balanced `{…}` expression is replaced by `"` + spaces + `"` of the same length before the HTML parse, so expression syntax never reaches tree-sitter-html. Component tags inside expressions are a known P1 limit.
- The shared `mask` lives in `mct-tree-sitter`. The merge step (drop each sub-parse's own whole-file `module`, add the file's `module`, remap `SymbolId`s and `RelationTarget.relation` indices) lives in `mct-lang-astro` and moves to `mct-tree-sitter` when Vue or Svelte becomes its second user.
- **Crate rule:** an embedding crate (`mct-lang-astro`, later `-vue`/`-svelte`) may take normal dependencies on leaf lang crates and use only their public API. Leaf crates never take normal dependencies on other lang crates, and there are no cycles. If js-ts picks its grammar by file extension, P1 adds a public dialect-selecting entry point to `mct-lang-js-ts`. That is a crate-local API, not core.
- A relation from an embedded region keeps the file's own language (`astro`). A cross-language link sets `language` (+ `path` when the import names an extension), per ADR-002.

*Why:* this reuses three fuzzed parsers and adds no grammar. Masking makes line mapping correct by construction.

### D5 MCP surface
No new tools. The only signature change lands in P0b.

| Tool | Change | Phase | Backward compatibility |
|---|---|---|---|
| `list_symbols` | The `kind` description lists `asset`, `model_node`, `material`, `animation`, `finding` | P0b | Description only. `kind` is already a free string. |
| `get_indexing_status` | New optional `dependency: string` (exact package name): lists each manifest that declares it, with its version | P0b (P3 tests it) | Additive. When omitted, output is unchanged. |
| `build_context_pack` | A definition of kind `asset` is printed without source lines (it is binary or JSON) | P0b | Behavior fix. Same shape. |
| `find_references`, `find_callers`, `find_calls`, `impact_analysis` | New `calls` rows for component tags (P2/P1) and `references` rows to assets (P5/P4) | P1, P2, P4, P5 | No param change. More rows of the existing shape. |
| `build_context_pack` | New role values `asset` (assets the packed symbol or its callees reference), `style` (CSS `rule`s referenced), `package` (manifest dependencies matching the file's bare imports) | P6 | Role names are free text in both text and TOON. Same columns. |
| `list_symbols` | The accessibility report is `list_symbols(path, kind="finding")`. Finding names: `pointer-without-keyboard`, `motion-without-reduced-motion`, `pointer-without-touch`. | P7 | No param change. One description line naming the findings. Limits go in the js-ts parser spec. |

*Why:* rules.md keeps the tool count small. Every change is either additive or a new row value in an existing shape.

### D6 Shared fixture
`crates/mct-mcp-server/tests/fixtures/portfolio-3d-app/`. P0b creates the part marked P0b. Each phase adds only its own files and at least one case in a new `crates/mct-eval/suite-portfolio-3d.json` (own baseline). P0b teaches `mct-eval` to run that second suite (dev-only crate).

| Path | Added by |
|---|---|
| `package.json` (astro, react, three, @react-three/fiber, @react-three/drei, tailwindcss) | P0b |
| `public/models/robot.gltf`, `public/models/robot.glb` (< 2 KB, one scene, two nodes, one material, one animation, one image URI), `public/textures/wood.png` (1×1), `public/hdr/studio.hdr` | P0b (asset nodes), P4 (contents) |
| `src/pages/index.astro`, `src/layouts/Base.astro`, `src/components/Card.astro` | P1 |
| `src/components/Scene.tsx`, `src/components/Robot.tsx` | P2, extended by P5 and P7 |
| `astro.config.mjs`, `tailwind.config.mjs`, `tsconfig.json` | P3 |
| `src/styles/global.css` | P7 |

### D7 Migration and reindex
- **No schema migration in any phase.** Kinds are TEXT. Assets and findings are `files`/`symbols` rows. Dependencies already have a table. A future need for a column requires a new ADR.
- **Older binary on a newer index:** the migration count is unchanged, so it opens normally with no `CONNECTION_CLOSED`. It shows new kinds under the generic heading (`assets`, `model_nodes`, …). It treats asset/`.astro` rows as unparseable files it has seen, and keeps them. A re-parse of a changed `.tsx` drops that file's JSX rows until the newer binary indexes it again.
- **Reindex:**

| Phase | Needs `reindex --force` once? | Why |
|---|---|---|
| P0b | no | Asset files were never indexed, so the next reindex adds them. |
| P1 | no | `.astro` files were never indexed. |
| P2, P5, P7 | **yes** | Unchanged `.tsx`/`.jsx`/`.astro` files are hash-skipped. |
| P4 | **yes** | Unchanged assets are hash-skipped. |
| P3, P6 | no | Manifests are re-read every run, and P6 only queries. |

Each phase that needs `--force` states it in CHANGELOG, as the Markdown literal change did.

*Why:* staying off migrations keeps every older binary working on the index and avoids the branch-switch failure.

### Phase map
"Core" means the `LanguageParser` trait or `SourceFile`/`ParsedFile`, `SymbolKind`/`RelationKind`, the SQLite schema, or a published MCP tool signature.

| Phase | Crates touched | Core change beyond P0b |
|---|---|---|
| P0b | `mct-core` (5 kinds, `resolve_reference_path`), `mct-index` (`assets.rs`, kind strings, fuzz target for the `.glb` chunk reader), `mct-tree-sitter` (`mask`), `mct-mcp-server` (`KIND_HEADINGS`, kind docs, `dependency` param, no source for `asset`), `mct-corpus` (`kind_str`), `mct-eval` (second suite), fixture | is the core change |
| P1 | new `mct-lang-astro`; `mct-lang-js-ts` (public dialect entry, if needed); registration in `mct-mcp-server` + `mct-cli`; fixture; eval | no |
| P2 | `mct-lang-js-ts`; fixture; eval | no |
| P3 | `mct-mcp-server` tests; fixture | no |
| P4 | `mct-index` (`gltf.rs`, fuzz target); fixture; eval | no |
| P5 | `mct-lang-js-ts`, `mct-lang-astro`; fixture; eval | no |
| P6 | `mct-mcp-server`, `mct-index` (queries); eval | no |
| P7 | `mct-lang-js-ts`, `mct-lang-astro` (template `class`); `mct-mcp-server` (one description line); fixture | no |

## Consequences
- P1–P7 become additive work in their own crates. The contract is settled once by P0b.
- Assets and glTF knowledge live in `mct-index` next to manifests. This is a second deliberate exception to "`mct-index` knows no grammar", limited to data formats with no tree-sitter grammar.
- Lang crates may now depend on lang crates, in one direction only (embedding → leaf).
- Known limits: no tsconfig aliases or custom `publicDir`; no component tags inside Astro `{…}` expressions; no meshes; no JSX props; in-root symlinked assets resolve only by their canonical path; findings are heuristics with documented false positives.

## Issue draft
**Title:** Web/3D foundation: asset nodes, five symbol kinds, reference-path resolver (ADR-003)

**Body:**
> Implements the cross-cutting part of ADR-003 (`docs/01-architecture/adrs/003-web-3d-indexing.md`) so the seven web/3D roadmap items (Astro, JSX, config, glTF, code→asset, context bundle, styles/a11y) are additive afterwards.
>
> Contract changes:
> - `mct-core`: `SymbolKind::{Asset, ModelNode, Material, Animation, Finding}` stored as `asset`, `model_node`, `material`, `animation`, `finding`. No new `RelationKind`. New pure fn `resolve_reference_path(from_file, spec)` (ADR-003 D3).
> - `mct-index`: an asset pre-pass for `glb gltf png jpg jpeg webp ktx2 hdr exr`. Each asset gets a `files` row (`language = "asset"`) plus one `asset` symbol. Images are only stat'ed. glTF JSON ≤ 8 MiB; the `.glb` JSON chunk is read via its header and the BIN chunk is never read. Bad input → `syntax_error` issue, never a panic. Includes a fuzz target.
> - `mct-tree-sitter`: `mask(contents, keep)` for embedded-language regions.
> - MCP: new optional `get_indexing_status.dependency`; `kind` descriptions list the new kinds; `build_context_pack` prints no source for `asset` definitions.
> - **No schema migration.** Older binaries keep opening the index. P2/P4/P5/P7 will each need one `reindex --force`.
>
> Out of scope: the feature phases P1–P7 themselves.
>
> Acceptance: resolver tests (relative, `/` → `public/`, `?url`, root-escaping `..`, absolute, scheme, missing file, symlink out of root); asset ingestion tests (caps, truncated/oversized `.glb`); an index built before this change opens and gains asset rows on a normal reindex; `scripts/unix/check.sh` passes.

#adr #architecture
