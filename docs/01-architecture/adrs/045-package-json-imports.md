# ADR-045: Node `package.json` `imports` as module evidence

## Status
Proposed (2026-11-21). Issue: [#563](https://github.com/Zubiarka8/mini-consumes-tokens/issues/563) (the Node `imports` row). Adds schema migration 28, on top of migration 27 ([ADR-044](044-framework-web-root.md)). Extends [ADR-043](043-project-config-module-evidence.md)'s alias evidence.

## Context
- `import { util } from "#lib/util"` names its file through the nearest `package.json` `imports` map (Node subpath imports). The relation was not even recorded as a module import, so the target looked unused.

## Decision
- **Parser:** a specifier starting with `#` is a module import, like `@/` (`module` = the specifier, `kind` = `Module`).
- **Tables:** migration 28 adds two tables, filled by the manifest scan.
  - `package_manifests(manifest_path, directory)`: one row per `package.json`, with or without `imports`.
  - `package_imports(manifest_path, position, pattern, target)`: the literal map, resolved to repository paths.
  - Both are swept like the other manifest tables.
- **View:** `ts_alias_candidates` gains a `UNION ALL` branch for `#` modules. Here, the inner compound is fine: the only outer `UNION` that matters is the one ADR-043 rules out, on `relation_candidates`. `relation_candidates` keeps one `SELECT`, and its alias guard becomes `@/` or `#`.
- **Scope:** the nearest `package.json` governs a file, as in Node. A nested package without `imports` stops an outer map from resolving its files.
- **Literal only:**
  - A key must start with `#` and have at most one `*`. Each target needs as many `*`.
  - A target must be `./`-relative with no `.` or `..` segment.
  - String, array and condition-object leaves all count. Every leaf that exists stays a candidate, so a fork is ambiguous rather than guessed.
  - A `.js`, `.mjs` or `.cjs` target is also stored without its extension. Then the TypeScript source it is emitted from matches.
- Migration 28 reparses JS/TS files.

## Not done
- `exports` maps and workspace package names (`@acme/ui` in a sibling folder).
- Vite/webpack `resolve.alias`, import maps, and the non-JS ecosystems in #563.
- `get_indexing_status` does not yet list `package.json` alias sources.

## Verification
- `mct-lang-js-ts/tests/depgraph.rs` › `package_json_imports_resolve_through_the_nearest_package`. It covers:
  - a wildcard key with a `.js` target resolves to its `.ts` source;
  - a condition object resolves;
  - a nested package without `imports` stays unresolved;
  - removing the map unresolves the import.

  It failed before the change (`left: None`).
- `indexer::package_import_tests::only_literal_package_relative_imports_are_kept`.
- `ambiguous_imports_do_not_make_file_imports_quadratic` stays under its bound.
