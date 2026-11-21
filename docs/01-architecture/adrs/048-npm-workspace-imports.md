# ADR-048: Bare JS/TS imports of workspace packages

## Status
Proposed (2026-11-21). Issue: [#563](https://github.com/Zubiarka8/mini-consumes-tokens/issues/563) (npm workspaces). Adds schema migration 31, on top of migration 30 ([ADR-047](047-cmake-include-directories.md)).

## Context
- `import { Button } from '@acme/ui'` names the workspace package whose `package.json` says `"name": "@acme/ui"`.
- The binding only carried the module name `ui`, matched against path segments. It missed a package in `packages/design-system/`, and it matched any file with `/ui/` in its path.

## Decision
- **Parser:** a named, default or namespace import binding from a bare specifier stores the package it names in `RelationTarget::package` and the new `relations.package` column. That is `@scope/name` or `name`, without the subpath.
  - Relative, absolute, `@/`, `#` and protocol (`node:`) specifiers store none.
  - No new relation is recorded. A binding already existed for every import; a bare `import 'react'` with no binding still records nothing.
- **Table:** `package_manifests` gains `name`, read from each `package.json`, plus an index on it.
- **View:** when some `package.json` in the repository has that name, a candidate must sit inside that package's directory and not inside a nested package. When no `package.json` has the name (`react`, `lodash`), the binding keeps the module-name rule unchanged.
  - This is the maintainer's choice of 2026-11-21: workspace names only; external packages are not modelled.
- Migration 31 reparses JS/TS files to fill `package`.

## CommonJS
- `const { a, b: c } = require('spec')` binds `a` and `c` like `import { a, b as c } from 'spec'`: same module name, same package, same view rules.
- Any other pattern (`const x = require(…)`, a computed argument) binds nothing, as before.

## Not done
- `exports`/`main` entry points of the package. Any file in the package counts, so a name defined twice inside the package stays ambiguous. Narrowing to the entry file needs re-exports (`export * from './button'`) followed first, or it would unresolve bindings that resolve today. Left out by maintainer's choice of 2026-11-21.
- Vite and webpack `@` aliases: done in [ADR-050](050-vite-at-alias.md). HTML import maps are not.
- Dynamic `import('@acme/ui')` bindings (`const { x } = await import(…)`).

## Verification
- `mct-lang-js-ts/tests/depgraph.rs` › `a_destructured_require_of_a_workspace_package_resolves_inside_that_package`. It failed before the change.
- `mct-lang-js-ts/tests/depgraph.rs` › `a_workspace_package_import_resolves_inside_that_package`.
  - Before the change it failed with `left: Unresolved`.
  - Without the exclusive clause it fails with `left: Ambiguous`, because `apps/web/src/ui/button.ts` also matches. The test file is kept as that trap.
  - `react` stays unresolved.
  - Renaming the package drops it from the candidates.
- `walker::imports::bare_package_tests`.
