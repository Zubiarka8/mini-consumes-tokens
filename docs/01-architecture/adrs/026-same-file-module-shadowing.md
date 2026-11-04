# ADR-026: A file's module is not a candidate shadowed by the type declared in it

## Status
Proposed (2026-10-31). Issue: [#272](https://github.com/Zubiarka8/mini-consumes-tokens/issues/272). Adds schema migration 13, which [ADR-003](003-web-3d-indexing.md) D7 does not cover. Amends the candidate rules of [ADR-002](002-qualified-relation-resolution.md) and follows the migration pattern of [ADR-005](005-markdown-alias-storage.md).

## Context
- Every file has a synthetic `module` symbol named after its stem. In PHP, Java, C# and TypeScript a file usually declares a type with the same name (`Order.php` declares `Order`).
- A relation's candidates are every symbol with the relation's `to_name` that satisfies the evidence the parser attached. A reference that no syntax pins to one kind (a parameter type, `Foo::class`, a Java `implements`, a JSX component) carries no `target_kind`, so both the module and the type are candidates and the relation reads `ambiguous among 2` although the file is the only target.
- Observed in the repository's corpora, in Java (`implements`), JS/TS (`references`) and PHP (`references`), and in a Laravel application (`find_references Order`).

## Decision
Migration 12 recreates `relation_candidates` with one more predicate. Every other predicate is unchanged.

```sql
AND NOT (s.kind = 'module' AND r.kind <> 'imports' AND r.target_kind IS NULL
         AND EXISTS (SELECT 1 FROM symbols o
                     WHERE o.file_id = s.file_id AND o.name = s.name
                       AND o.parent IS s.parent AND o.kind <> 'module'))
```

- **Scope.** Only the module, only for a relation that is not an `imports` and has no `target_kind`. A relation that pins `target_kind` (a Markdown note link, an import) is untouched.
- **Safety.** The dropped module is replaced by `o`, which satisfies every predicate the module satisfied: the same file gives the same language, `target_path` and `target_module`; the same name gives the same `to_name` match; the same `parent` gives the same `qualifier` and `module` outcomes; `o.kind <> 'module'` also satisfies `member` and the `calls` rule, which only exclude modules. A relation therefore never goes from some candidates to none. Without the `parent` clause a nested module whose `parent` satisfies a qualifier would lose its only candidate.
- **Not a tie-break by order.** A same-named symbol in a different file stays a candidate, so a real ambiguity is still reported. A relation whose only candidate is a module still resolves.
- **Reindex.** Like the earlier view changes, the migration empties `files.content_hash` so every file is reparsed.

## Verification
`crates/mct-index/src/schema/module_shadow_tests.rs` enumerates relation shapes (kind, qualifier, `target_path`, `target_language`, `module`, `target_module`, `member`, `target_kind`) against module and alternative placements (kind, parent, file, language) on the real view, with the old view rebuilt from the same SQL minus the clause. It asserts that no relation loses its last candidate, that only a module is dropped and only when the alternative stays, and that `imports` and kind-pinned relations are unchanged. Regression tests in the Java, JS/TS and PHP corpora and the Laravel fixture pin the three observed cases.

## Alternatives rejected
- Drop modules from every relation without `target_kind`: passes the suite but turns a relation whose only candidate is a module into an unresolved one, and relies on every future parser pinning the kind.
- Same file and name without `parent`: unsafe for nested modules.
- One relation per possible kind in a parser: invents relations.
- Rename the module: breaks `imports`.

## Relation to ADR-003 D7
D7 forbids schema migrations in the web/3D phases. This is a separate change with its own issue, and `crates/mct-index/tests/assets.rs` pins the schema version (12 with this ADR, 13 with [ADR-027](027-possible-entry-points.md)).
