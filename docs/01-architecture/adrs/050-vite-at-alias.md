# ADR-050: The `@` alias of a Vite or webpack config

## Status
Proposed (2026-11-21). Issue: [#563](https://github.com/Zubiarka8/mini-consumes-tokens/issues/563) (bundler aliases). Adds schema migration 32, on top of migration 31 ([ADR-048](048-npm-workspace-imports.md)).

## Context
- Vite projects often declare `resolve: { alias: { '@': path.resolve(__dirname, './src') } }` and never repeat it in `tsconfig.json` `paths`.
- `@/x` imports in those projects found no target: only tsconfig `paths` fed `ts_alias_candidates`.
- The JS/TS plugin records an alias import only for `@/…` specifiers, so `@` is the one alias worth reading.

## Decision
- **Reader** (`mct-index/src/vite.rs`): in `vite.config.*` and `webpack.config.*` (`js`, `ts`, `mjs`, `mts`, `cjs`, `cts`), finds `alias:` as an object (`'@': value`, also `'@/'`) or an array (`{ find: '@', replacement: value }`).
  - `value` must hold exactly one string literal, a path inside the config's directory. `/src` counts as `src`, since Vite resolves it from its root.
  - Around it, only `path.resolve`/`path.join` with `__dirname`, or `fileURLToPath(new URL(…, import.meta.url))`, are allowed. Anything else, including a template with `${}` or `..`, is skipped.
  - Comments are ignored.
- **Table:** `vite_aliases(file_id, directory, pattern, target)`, one `@/*` → `<dir>/*` row per config. It is written each time the config is reparsed as a source file, and deleted with its `files` row (`ON DELETE CASCADE`). No manifest sweep is needed.
- **View:** `ts_alias_candidates` gains a third branch. The nearest Vite config containing the importing file applies, like the nearest tsconfig.
  - Maintainer's choice of 2026-11-21: merged, tsconfig first. The Vite branch is dropped when the importing file's nearest tsconfig maps the same pattern.
- Migration 32 creates the table, recreates the view and reparses Vite configs.

## Not done
- Aliases other than `@` (`~`, `@components`): the JS/TS plugin records no alias import for them.
- HTML import maps: they map bare specifiers, which #616 already reads as workspace packages; mixing the two needs its own rule.
- webpack configs split across files (`webpack.common.js` + `merge`).
- A Vite `root` other than the config's directory.

## Verification
- `mct-index` unit tests `vite::tests` (accepted and skipped forms).
- `mct-lang-js-ts/tests/depgraph.rs` › `a_webpack_at_alias_resolves`.
- `mct-lang-js-ts/tests/depgraph.rs` › `a_vite_at_alias_resolves_and_a_tsconfig_mapping_wins` (new; before this change a Vite config was not read).
