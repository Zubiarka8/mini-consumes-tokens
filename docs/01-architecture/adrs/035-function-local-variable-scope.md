# ADR-035: A function's variable is a candidate only from its own file

## Status
Proposed (2026-11-10). Issue: [#320](https://github.com/Zubiarka8/mini-consumes-tokens/issues/320) (D4). Adds schema migration 15, which [ADR-003](003-web-3d-indexing.md) D7 does not cover. Amends the candidate rules of [ADR-002](002-qualified-relation-resolution.md) and keeps the clause of [ADR-026](026-same-file-module-shadowing.md). ADR-034 (enriched symbol model, issue #321) moves to migration 16.

## Context
- Parsers index some variables declared inside a function: Rust parameters and `let` bindings, Lua `local`s and loop variables, C++ locals. Their `parent` is the enclosing function's name.
- `relation_candidates` keeps every same-named, same-language symbol the parser's evidence does not exclude. A variable of a function in one file is therefore a candidate for a relation from any other file, and a lone one is reported `resolved`.
- Observed: in the labelled suite, a Lua `speed()` in `hud.lua` (a host global) resolves to `local speed` inside `step` in `physics.lua`. In this repository's index, a Rust `step()` call lists other files' `let step` bindings among its candidates.
- Lexical scope makes such a variable unreachable from another file in every language that indexes one. Bash and PowerShell index only file-level variables, so their dynamic scoping is not affected.

## Decision
Migration 15 recreates `relation_candidates` with one more predicate. Every other predicate is unchanged.

```sql
AND (sf.id = cf.id
     OR s.kind <> 'variable'
     OR s.parent IS NULL
     OR NOT EXISTS (SELECT 1 FROM symbols p
                    WHERE p.file_id = s.file_id AND p.name = s.parent
                      AND p.kind IN ('function', 'method'))
     OR EXISTS (SELECT 1 FROM symbols p
                WHERE p.file_id = s.file_id AND p.name = s.parent
                  AND p.kind NOT IN ('function', 'method', 'variable',
                                     'constant', 'field')))
```

- **Evidence, not names.** The variable is dropped only when its own file proves the parent is a function or method, and no container of that name in that file (a class, module, struct…) could be the parent. A same-named variable, constant or field is no container: `local remaining = limiter:remaining()` elsewhere in the file does not keep `Limiter:remaining`'s `local now` a candidate. A field of a class, a module-level variable and a Lua global first assigned in a function (no `parent`) stay candidates from anywhere.
- **Same file unchanged.** A relation from the variable's own file keeps it, so scoping inside a file stays the parser's job.
- **No reparse.** The predicate reads only existing columns, so the migration does not empty `files.content_hash`.

## Verification
- `schema::tests::function_local_variable_migration_scopes_candidates_to_their_file` upgrades a version-14 database holding data. It checks the five cases above (including a same-named variable that is no container) and that no file is queued for reparse.
- `tests/read_only_open.rs` checks that an index with the migration pending is refused read-only and left unchanged.
- `module_shadow_tests.rs` still enumerates the ADR-026 clause on the new view.
- The D4 label of `crates/mct-eval/suite-resolution.json` goes from wrong to unresolved.

## Alternatives rejected
- **Drop every variable with a parent from cross-file candidates.** A class or struct field indexed as `variable` would lose real edges.
- **A `scope` column filled by the parsers.** Needs a `LanguageParser` output change and a full reparse. It belongs to the enriched symbol model (ADR-034, #321), which will also cover Lua file-level `local`s. Today those cannot be told apart from globals.
- **Ban cross-file edges by path (production → fixture).** Hides real edges; rejected in #320.

## Relation to ADR-003 D7
D7 forbids schema migrations in the web/3D phases. This is a separate change with its own issue, and `crates/mct-index/tests/assets.rs` pins the schema version at 15.
