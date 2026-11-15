# ADR-037: A type-qualified relation reaches a method its direct supertype declares

## Status
Proposed (2026-11-15). Issue: [#479](https://github.com/Zubiarka8/mini-consumes-tokens/issues/479). Adds schema migration 18, which [ADR-003](003-web-3d-indexing.md) D7 does not cover. Amends the qualifier rule of [ADR-002](002-qualified-relation-resolution.md). Keeps the clauses of [ADR-026](026-same-file-module-shadowing.md), [ADR-035](035-function-local-variable-scope.md) and [ADR-036](036-not-field-relation-target.md).

## Context
- A parser that proves a receiver's type sets `qualifier` (`$this->where(..)` in `BookRepository`, `repo.SaveAsync()` on a `StockRepository`). `relation_candidates` then keeps only symbols whose `parent` is that type.
- A method the type inherits has the supertype as its parent. So the relation had no candidate and stayed `unresolved`, although the index already stores the `extends`/`implements` relation that names the supertype.
- Measured on this repository's index (read-only copy): 38 such relations are unresolved, where the type's direct supertype, recorded the same way, declares a symbol of that name. They are PHP 19, C# 16, Java 2 and TS 1, all in the parser corpora.

## Decision
Migration 18 recreates `relation_candidates` with one more way to satisfy the qualifier. Every other predicate is unchanged.

```sql
OR (EXISTS (SELECT 1 FROM relations ir
            JOIN symbols t ON t.id = ir.from_symbol_id
            JOIN files tf ON tf.id = t.file_id
            WHERE ir.kind IN ('extends', 'implements')
              AND ir.to_name = s.parent
              AND t.name = r.qualifier
              AND tf.language = sf.language)
    AND NOT EXISTS (SELECT 1 FROM symbols own
                    JOIN files ownf ON ownf.id = own.file_id
                    WHERE own.name = r.to_name
                      AND own.parent = r.qualifier
                      AND ownf.language = sf.language))
```

- **Evidence, not a name.** The supertype must be the target of an inheritance relation stored for a type named like the qualifier, in the candidate's language. A same-named method of an unrelated class is still no candidate.
- **An override shadows.** If the type declares a member of that name itself, only that member is a candidate. An existing `resolved` never becomes `ambiguous`.
- **One hop.** A grandparent's method stays `unresolved`. Walking further needs "the nearest declaring ancestor wins", which is a separate step.
- **No reparse.** The view reads only columns the index already has, so stored rows are unchanged and `files.content_hash` is kept.

## Verification
- `schema::tests::inherited_member_migration_reaches_only_a_recorded_direct_supertype` upgrades a version-17 database holding data. An inherited method gains its candidate. The type's own method shadows the supertype's. A grandparent's method and another language's supertype give none. No file is queued for reparse.
- `crates/mct-index/tests/relation_resolution.rs`: the same three rules through a full reindex.
- This repository (`init --force`, a copy of the index built from `main` compared with one built from this change): 34 relations go from `unresolved` to `resolved` (C# 16, PHP 15, Java 2, TS 1), and nothing else changes state. Each one goes to the supertype's method, for example `InMemoryRepository::where`, `Jdbc.findAll` and `Observable.subscribe`. Dead code is unchanged.
- The other 4 of the 38 are PHP trait methods (`log`, `audit`). They keep a `target_module` (the caller's namespace) that the trait's file does not declare, so they stay `unresolved`.

## Alternatives rejected
- **Resolve inherited members in each parser.** A parser sees one file, and the supertype is often declared in another.
- **Any symbol named like the method whose parent is any type.** That is a match by name, and it is ruled out.
- **A transitive walk now.** It needs shadowing at every level and a depth cap, and on this repository a direct supertype already covers every case found.

## Relation to ADR-003 D7
D7 forbids schema migrations in the web/3D phases. This is a separate change with its own issue, and `crates/mct-index/tests/assets.rs` pins the schema version at 18.
