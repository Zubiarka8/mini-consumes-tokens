# ADR-046: Renamed Cargo path dependencies as module evidence

## Status
Proposed (2026-11-21). Issue: [#563](https://github.com/Zubiarka8/mini-consumes-tokens/issues/563) (the Rust "package renames" row). Adds schema migration 29, on top of migration 28 ([ADR-045](045-package-json-imports.md)). Extends [ADR-043](043-project-config-module-evidence.md)'s Cargo evidence.

## Context
- `store = { package = "grep-store", path = "../libs/searcher" }` lets the dependent crate write `use store::Searcher`.
- `store` matches neither the target's directory nor its library name, so the relation stayed unresolved.

## Decision
- **Table:** `cargo_renames(manifest_path, directory, alias, target_directory)`.
  - It holds each dependency in `[dependencies]`, `[dev-dependencies]` or `[build-dependencies]` that sets both `package` and `path`.
  - The alias has dashes read as underscores.
  - The path is resolved from the manifest's directory. A path that leaves the repository is dropped.
  - Filled by the manifest scan, which reads every manifest on every scan. So an existing index gains the rows on its next reindex. Swept like the other manifest tables.
- **View:** `relation_candidates` gains one Rust module-evidence branch. A module equal to an alias matches symbols under the target directory, only for files of the declaring package:
  - Nearest manifest: a nested package excludes both the caller side and the target side.
  - Every other predicate is unchanged.

## Not done
- A registry rename (`package` with `version`, no `path`): its source is outside the repository.
- `workspace = true` dependencies that inherit a rename from `[workspace.dependencies]`.
- `[patch]` and `[target.'cfg(…)'.dependencies]`.

## Verification
- `relation_resolution.rs` › `a_renamed_path_dependency_resolves_inside_the_dependent_package`.
  - It failed before the change, with `left: Unresolved`.
  - The alias resolves only to the path dependency, and only from the declaring package.
  - Removing the rename unresolves it.
- `manifests::rust::rename_tests::only_renamed_path_dependencies_are_kept`.
