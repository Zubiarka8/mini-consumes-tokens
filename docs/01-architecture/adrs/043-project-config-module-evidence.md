# ADR-043: Module evidence from Cargo package names and tsconfig paths

## Status
Proposed (2026-11-20). Issues: [#374](https://github.com/Zubiarka8/mini-consumes-tokens/issues/374), [#563](https://github.com/Zubiarka8/mini-consumes-tokens/issues/563) (TypeScript/JavaScript part only). Adds schema migration 26, on top of migration 25 ([ADR-042](042-file-scoped-symbols.md)). Amends the module-evidence rule of [ADR-002](002-qualified-relation-resolution.md).

## Context
- Module evidence matches path segments: `mct_core` matches `crates/mct-core/`. A Rust package whose directory has another name (`crates/searcher/` named `grep-searcher`) gets no edge.
- `import X from "@/components/search"` names its file through `compilerOptions.paths`. The relation stays unresolved, its target looks unused, and impact analysis misses the importer.
- A parser sees one file, so it cannot read either config.

## Decision
Migration 26 adds four tables, filled by the manifest scan that already visits these files:
- `cargo_packages(manifest_path, directory, crate_name)`: the `[lib] name`, or else `[package] name`, with dashes read as underscores.
- `ts_configs(manifest_path, directory, error)`, `ts_paths(manifest_path, position, pattern, target)` and `ts_config_parents(manifest_path, parent_path)`: for each `tsconfig.json`/`jsconfig.json`, its `paths` after `extends`, resolved to repository paths.

The view `relation_candidates` gains two kinds of evidence. Every other predicate is unchanged.
- **Rust:** a module equal to a recorded crate name matches the symbols under that package's directory, and not under a nested package.
- **TS/JS:** an import whose specifier starts with `@/` is offered the module symbol of each file the nearest config's `paths` map it to: the exact path, `.ts`, `.tsx`, `.js`, `.jsx`, `/index.ts` or `/index.js`. Several existing targets stay candidates, so the relation is ambiguous rather than guessed.

Rules:
- **Literal config only.** `paths` patterns and targets need exactly one `*`. An absolute target is ignored.
- **`baseUrl` and `paths` resolve from the config that declares them**, as TypeScript does. A config's own value replaces an inherited one. `extends` follows relative parents (string or array; with an array, the last parent that declares a value wins). It goes at most 8 levels deep and breaks cycles. A package `extends` (`@tsconfig/node18`) is skipped.
- **The nearest config wins.** A file under `web/` uses `web/tsconfig.json`, not the root one. A config that cannot be parsed stays as the nearest config, with no alias and its error stored, so an outer config never resolves files it does not govern.
- **Stale data:** a changed or deleted config re-reads every indexed config that extends it (`ts_config_parents`). A config with an unchanged result stops the walk. Migration 26 reparses JS/TS files, because the parser now emits `@/` imports as module relations.
- **No compound view.** The alias candidates join the `s.id IN (...)` generator of the single `SELECT`, behind a cheap `substr(r.module, 1, 2) = '@/'` guard. An outer `UNION` made the view compound, so `direct_file_imports` could no longer stop at its `LIMIT 2`. On its 200-directory test this took 9.16M VM steps against a 600k bound.
- `get_indexing_status` (MCP and CLI) lists every alias source with its alias-target count, or why it could not be read.

## Not done (rest of #563)
- Vite/webpack `resolve.alias`, `package.json` `imports`/`exports` and workspaces, import maps, Python, Go `replace`, Gradle/Maven, `.csproj`, `composer.json`, CMake include directories and Lua `package.path`.
- Prefixes other than `@/`: the parser only emits module evidence for `@/` specifiers, because a bare specifier (`react`) would otherwise match same-named local folders.
- A parent config that is not named `tsconfig.json`/`jsconfig.json` (`tsconfig.base.json`) is read when its child is, but a change to it alone waits for the next full reindex.

## Verification
- `mct-index/tests/relation_resolution.rs`: `cargo_library_names_resolve_only_inside_the_declaring_package`, `cargo_library_evidence_does_not_cross_nested_packages`, `duplicate_cargo_library_names_remain_ambiguous`.
- `mct-lang-js-ts/tests/depgraph.rs`: the fixture alias, comments and trailing commas, two existing targets kept as candidates, an edited and a deleted config, `an_extended_config_supplies_paths_relative_to_itself` (RED without `extends`), `own_paths_override_inherited_ones_and_cycles_or_packages_give_none`, `indexing_status_lists_each_alias_source_and_an_unreadable_one_resolves_nothing`.
- `dependency_graph::tests::ambiguous_imports_do_not_make_file_imports_quadratic` stays under its bound.
- On this repository (`mct-cli init` of the working tree; each table emptied in turn on a copy of the index, 163,601 relations):
  - **TS:** 21 `@/` imports go from unresolved to resolved. 20 of them are in `website/`, and each resolves to the file its alias names. No other relation changes state.
  - **Rust:** 44 packages recorded; no relation changes state, as #374 predicted, because no imported package here has a renamed directory.
  - **Cost:** a full scan of `relation_candidates` took 25.1 s, against 23.5 s for migration 25's index of a slightly older tree (161,493 relations), about +5% per relation.
