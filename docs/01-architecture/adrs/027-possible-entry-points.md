# ADR-027: Possible framework entry points are stored evidence, not liveness

## Status
Proposed (2026-10-31). Issue: [#273](https://github.com/Zubiarka8/mini-consumes-tokens/issues/273). Adds schema migration 14, which [ADR-003](003-web-3d-indexing.md) D7 does not cover. Related: framework overlays in #268. Follows the migration pattern of [ADR-005](005-markdown-alias-storage.md) without reusing its table.

## Context
- A Laravel listener found only by event discovery (`app/Listeners/NotifyWarehouse.php` with `handle(OrderPlaced $e)`) has no reference in source, so `find_dead_code` reports it as a candidate. One named in source (`Event::listen(OrderPlaced::class, AuditOrder::class)`) is not reported.
- Whether the framework loads the class depends on things the source does not prove: discovery is the default in Laravel 11, 12 and 13 but `withEvents(discover: [...])` can point it elsewhere, `event:cache` freezes the result, and 13.x lets `shouldBeDiscovered()` opt a class out at run time.
- The indexer has no place for per-symbol evidence, and `symbol_aliases` holds search names only.

## Decision
- **Parser output.** `ParsedFile.entry_points: Vec<EntryPointHint { symbol, framework, rule, evidence, requires }>`, with `requires: Option<PackageRequirement { manifest_path, package, min_major }>`. A parser records a convention it can read from one file; it does not read manifests.
- **Storage.** Migration 13 creates `symbol_entry_points (symbol_id REFERENCES symbols ON DELETE CASCADE, framework, rule, evidence, requires_manifest, requires_package, requires_min_major, PRIMARY KEY (symbol_id, framework, rule))` and empties `files.content_hash` so every file is reparsed. Re-indexing a file replaces its rows through the cascade.
- **Read-time requirement.** The indexer stores the requirement as declared. `scan_dead_code` evaluates it against `dependencies(manifest_path, name, version)`: a missing dependency or a declared major below `min_major` hides the note; a major that cannot be read (`*`, `dev-main`) keeps it as possible. A manifest-only edit therefore changes the output without reparsing source.
- **Output.** The candidate stays a candidate, with the same id and review state. `find_dead_code` adds a `possible_entry_point` column, the CSV report adds the same column, and the dashboard adds `possibleEntryPoints`, each only when some candidate has a note. The wording is `possible entry point ({framework}, {rule}): {evidence}; nothing in the source references it and the framework may load it by convention. Not proven.` and never says the framework loads it.
- **Laravel rule `event-listener-discovery`.** A non-abstract class under `<root>app/Listeners/`, with a public non-static method named `handle*` or `__invoke*` whose first parameter is typed with a class-like name (union types allowed; `self`, `static`, `parent`, `object`, `mixed`, `iterable`, `callable` excluded). It requires `<root>composer.json` to declare `laravel/framework` with major 11 or later. The root is the path prefix before `app/Listeners/`; it is never guessed from an ancestor, so a monorepo application without its own `composer.json` gets no hint.

## Consequences
- A project that is not a Laravel application, or declares an older major, sees today's output unchanged.
- Listeners in directories configured with `withEvents`, and conventions beyond listeners (commands, policies), are not marked.
- Adding a framework rule is a parser change plus a requirement; the table, the scan and the three outputs stay as they are.

## Alternatives rejected
- Store the hint in `symbol_aliases`: aliases are search names, and a hint there would be searchable as a symbol name.
- Treat a matching class as alive: states liveness the source does not prove.
- Mark by path and method alone: marks any PHP project's `app/Listeners` class.
- Exclude `storage/framework` and `bootstrap/cache` as built-ins: a path cannot show that a project is a Laravel application; documented as `.mctignore` lines instead.

## Verification
`crates/mct-lang-php/tests/listeners.rs` (parser rule), `crates/mct-index/tests/entry_points.rs` (persistence, requirement, monorepo roots, manifest-only edit, migration from version 13), `crates/mct-mcp-server/tests/possible_entry_points.rs` (tool output), plus CSV, dashboard and Laravel fixture tests.
