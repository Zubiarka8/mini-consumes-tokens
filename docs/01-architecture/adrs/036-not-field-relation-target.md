# ADR-036: A parser can rule out a field as a relation's target

## Status
Proposed (2026-11-12). Issue: [#412](https://github.com/Zubiarka8/mini-consumes-tokens/issues/412). Adds schema migration 17, which [ADR-003](003-web-3d-indexing.md) D7 does not cover. Amends the candidate rules of [ADR-002](002-qualified-relation-resolution.md) and keeps the clauses of [ADR-026](026-same-file-module-shadowing.md) and [ADR-035](035-function-local-variable-scope.md).

## Context
- `relation_candidates` keeps every same-named, same-language symbol the parser's evidence does not exclude. A call with no evidence therefore reaches a `field` by name alone, and a lone candidate is reported `resolved`.
- Whether a call can reach a field depends on the language and the call. Python `self.key(row)` calls a callable attribute. `CONFIG.items()` on a dict does not call a `Page.items` field of some class.
- `RelationTarget` could not say "any kind but `field`": `kind` admits one kind, and `member` admits only methods and always reports ambiguity.
- Observed: indexing `self.x = …` as a field of the class (Python) resolved 19 unresolved calls in this repository's snapshot. 6 were wrong, all `X.items()` on a module-level dict, resolved to `Page.items`.
- A resolver-wide rule ("a call with no evidence never reaches a field") would move cpp 28, csharp 8, JS/TS 44, kotlin 14, lua 11, rust 583 and python 9 calls. Those counts were not checked for correctness, so the rule is too wide.

## Decision
`RelationTarget` gains `not_field: bool`, stored as `relations.not_field`. Migration 17 adds the column (default 0) and recreates `relation_candidates` with one more predicate. Every other predicate is unchanged.

```sql
AND (r.not_field = 0 OR s.kind <> 'field')
```

- **The parser decides.** Only a parser that has a reason sets the flag. No other language changes.
- **Python sets it** on a call `v.m()` whose receiver class is not proven, unless `v` is the method's own `self`: `self` may hold a callable field. A proven receiver class (`qualifier`) and a builtin receiver (`external`) don't need it.
- **Reparse.** Existing rows read 0 and keep their candidates. The migration empties `files.content_hash` so parsers that set the flag can.

## Verification
- `schema::tests::not_field_migration_rules_out_only_fields` upgrades a version-16 database holding data. An existing row keeps its field candidate, every file is queued for reparse, and a flagged row keeps the method and drops the field.
- `crates/mct-lang-python/tests/self_fields.rs` indexes `CONFIG.items()` (unresolved, not `Page.items`) and `self.key(row)` (resolved to the field).
- `crates/mct-lang-python/tests/name_evidence.rs` lists which calls carry the flag.

## Alternatives rejected
- **A resolver-wide rule.** Moves calls in every language with no evidence that the moves are right (see Context).
- **`kind` set to `method`.** `mod.Class()` and `mod.func()` are also attribute calls, so one kind is not enough.
- **`member` on every unproven attribute call.** A module-level name may hold a module, so the target can be a function or a class, and a single candidate would never resolve.

## Relation to ADR-003 D7
D7 forbids schema migrations in the web/3D phases. This is a separate change with its own issue, and `crates/mct-index/tests/assets.rs` pins the schema version at 17.
