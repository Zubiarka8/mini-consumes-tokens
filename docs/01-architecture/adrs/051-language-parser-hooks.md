# ADR-051: Optional `LanguageParser` hooks for project-rooted paths and file classes

## Status
Proposed (2026-11-27). PR: [#628](https://github.com/Zubiarka8/mini-consumes-tokens/pull/628) (GDScript).

## Context
- CONTRIBUTING step 4: a new language changes nothing in `mct-core`, `mct-index`, `mct-cli` or `mct-mcp-server` beyond its wiring.
- GDScript needed four things the shared crates did not offer. The first version of #628 named Godot in them:
  - `res://` paths are relative to the directory holding `project.godot`, which a parser cannot see (`resolve_godot_paths` in `indexer.rs`).
  - `project.godot` is both a dependency manifest (editor plugins) and a symbol source (autoloads). The indexer had a `project.godot` branch that parsed autoloads and wrote them as `gdscript` rows.
  - A `.gd` file is itself one class. Its skeleton must list the class's members, not one collapsed class (`gdscript_skeleton` in `server.rs`).
  - A scene, a text resource, an autoload or `extends "x.gd"` names a script by its file (`hero`), while the script's class is named by `class_name Hero`. The parser of the scene cannot see that name, so the class looked unused to `find_dead_code`.

## Decision
- **`LanguageParser::rooted_paths() -> Option<RootedPaths>`**, default `None`. `RootedPaths { scheme, marker }`: a relation-target path starting with `scheme` is relative to the nearest directory, from the file's own up to the repository root, that contains `marker`. With no marker, the root is the repository root. The indexer rewrites such paths after parsing (`resolve_rooted_paths`). GDScript returns `res://` / `project.godot`.
- **A manifest a parser claims is also a source file.** After a manifest's dependencies are written, the indexer looks up the file's extension in the registry. If a parser claims it, the file goes through the ordinary source path: hash skip, UTF-8 check, parse, write. The result is `Seen::ManifestAndSource`. GDScript claims the `godot` extension and parses autoloads itself (`mct-lang-gdscript/src/project.rs`). The dependency half (`manifests/godot.rs`, editor plugins) stays with the other manifests.
  - Consequence: an invalid-UTF-8 `project.godot` is a syntax issue like any source file. Its symbols are dropped and its dependency list keeps its last good value. The first version kept both and reported a read failure.
- **`LanguageParser::file_is_class() -> bool`**, default `false`. When `true`, `get_file_skeleton` shows the file's top-level class's signature as the header, then each member's signature or declaration line, with the body placeholder for a multi-line method or class (`format::class_file_skeleton`). GDScript returns `true` and records the script class's signature (`class_name X extends Y`).
- **A relation that pins its target's file may name the symbol by an alias** (migration 33). Before, a parser-declared alias fed module lookups only. Now an alias also offers its symbol when the relation sets `target_path`; the pin still restricts the candidate to that file. `referenced_symbols` counts the resolved target of such a relation, so `find_dead_code` sees the use. GDScript records the file name as an alias of a `class_name` class. The migration rebuilds `relation_candidates` and reparses GDScript files; other rows are unchanged.
- GDScript also claims `.tscn` and `.tres`: the parser reads them as text sections (`mct-lang-gdscript/src/resource.rs`), with no new hook.

## Not done
- A data-only rule for the skeleton (a class whose extent is the whole file). It would also change Python files holding a single class.
- Several markers or schemes per language.
- Inherited members through an alias: the ancestor walk in `relation_candidates` follows `extends` targets by name, so a member reached through an autoload, or `extends "x.gd"`, of a script that declares another `class_name` stays unresolved.

## Verification
- `mct-index/tests/parser_hooks.rs`, with a fake parser:
  - `a_manifest_a_parser_claims_is_also_a_source_file`: dependencies and symbols together, hash skip, edit and delete.
  - `invalid_utf8_in_a_claimed_manifest_is_a_syntax_error_like_any_source`.
  - `rooted_paths_start_at_the_nearest_marker_directory`.
- `mct-index/src/schema.rs` `path_pinned_alias_migration_offers_an_alias_in_the_pinned_file_only`.
- `mct-lang-gdscript/tests/project.rs`: autoload parsing through the public parser.
- `mct-lang-gdscript/tests/showcase.rs`: a `class_name` script attached only in a scene is referenced and not dead code.
- `mct-lang-gdscript/tests/global_receivers.rs`: `Game.start()` through a `project.godot` autoload still resolves end to end.
- `mct-mcp-server/tests/gdscript.rs`: the skeleton shows `class_name WorkshopPlayer` and the member signatures, without bodies.
