# ADR-042: A symbol only its own file can name

## Status
Proposed (2026-11-20). Issue: [#446](https://github.com/Zubiarka8/mini-consumes-tokens/issues/446). Adds schema migration 25, on top of migration 24 ([ADR-041](041-top-level-relation-target.md), PR #601). Amends the candidate rules of [ADR-002](002-qualified-relation-resolution.md).

## Context
- Some declarations are visible only in the file that declares them. A C# `using Alias = Target;` is visible in its file or namespace block only.
- Name matching in `relation_candidates` is repository-wide. A prototype that indexed the alias as a `type_alias` turned a resolved `new Money()` in another file ambiguous between the class and the alias. So the C# parser emitted no alias symbol.
- `RelationTarget` can pin a relation to a path, but nothing kept a *symbol* out of other files' candidate sets.

## Decision
`ParsedFile` gains `file_scoped: Vec<SymbolId>`, the symbols of the file that only the file can name. The indexer stores it as `symbols.file_scoped`. Migration 25 adds the column (default 0), recreates `relation_candidates` with one more predicate and empties `files.content_hash` so every file is reparsed. Every other predicate is unchanged.

```sql
AND (s.file_scoped = 0 OR sf.id = cf.id)
```

- **A list on `ParsedFile`, not a field of `SymbolRecord`.** No parser that leaves it empty changes. `SymbolRecord` is built by struct literal in 29 places.
- **Only candidate matching.** `find_symbol`, the overview and dead code still list the symbol: it is a real definition.
- **C# marks its non-global `using` aliases.** A `global using` alias is visible project-wide and stays no symbol. A use of the alias in its file (`new Alias()`, `Alias.M()`) still names the target.
- **Next users:** C# `file` types, C `static` functions and Kotlin/TS file-private helpers can be listed the same way, each with its own evidence.

## Verification
- `schema::tests::file_scoped_migration_offers_a_symbol_only_to_its_own_file`: an existing row is not file-scoped, a flagged symbol is a candidate only from its own file, and every file is queued for reparse.
- `mct-lang-csharp/tests/using_alias.rs`, `an_alias_is_a_file_scoped_symbol_other_files_never_match`: the alias is a `type_alias` that `find_symbol` lists, and another file's `new Money()` stays resolved to the class. Without the predicate it is ambiguous with the alias, as in the issue's prototype.
- On this repository (`mct-cli init`, migration 24 vs 25, 1,198 C# relations): 3 alias symbols. 4 same-file `Table` type uses go from unresolved to resolved to their alias, and a same-file `Money` reference gains its alias as a candidate. No other file's relation changed.
