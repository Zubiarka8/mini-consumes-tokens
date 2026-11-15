# ADR-037: A type-qualified relation reaches a method its type inherits

## Status
Proposed (2026-11-15). Issue: [#479](https://github.com/Zubiarka8/mini-consumes-tokens/issues/479). Adds schema migration 18, which [ADR-003](003-web-3d-indexing.md) D7 does not cover. Amends the qualifier rule of [ADR-002](002-qualified-relation-resolution.md). Keeps the clauses of [ADR-026](026-same-file-module-shadowing.md), [ADR-035](035-function-local-variable-scope.md) and [ADR-036](036-not-field-relation-target.md).

## Context
- A parser that proves a receiver's type sets `qualifier` (`$this->where(..)` in `BookRepository`, `repo.SaveAsync()` on a `StockRepository`). `relation_candidates` then keeps only symbols whose `parent` is that type.
- A method the type inherits has the supertype as its parent. So the relation had no candidate and stayed `unresolved`, although the index already stores the `extends`/`implements` relation that names the supertype.
- Measured on this repository's index (read-only copy): 38 such relations are unresolved, where the type's direct supertype, recorded the same way, declares a symbol of that name. They are PHP 19, C# 16, Java 2 and TS 1, all in the parser corpora.

## Decision
Migration 18 recreates `relation_candidates` with one more way to satisfy the qualifier. Every other predicate is unchanged.

```sql
OR (EXISTS (SELECT 1 FROM relations sr            -- s.parent is someone's supertype
                WHERE sr.to_name = s.parent AND sr.kind IN ('extends', 'implements'))
    AND NOT EXISTS (/* the qualifier type declares r.to_name itself */)
    AND s.parent IN (
        WITH RECURSIVE ancestors(name, depth) AS (
            SELECT r.qualifier, 0
            UNION
            SELECT ir.to_name, a.depth + 1      -- an extends/implements relation of a
            FROM ancestors a ...                 -- type named a.name, same language
            WHERE a.depth < 8)
        SELECT a.name FROM ancestors a
        WHERE a.depth > 0
          AND NOT EXISTS (/* a nearer level declares r.to_name */)))
```

- **Evidence, not a name.** Each supertype must be the target of an inheritance relation stored for a type of that name, in the candidate's language. A same-named method of an unrelated class is still no candidate.
- **The nearest declaration wins.** The type's own members are level 0. An ancestor's method is a candidate only when no nearer level declares a member of that name, so an override shadows what it inherits, and an existing `resolved` never becomes `ambiguous`. Two ancestors at the same level that both declare the method leave the relation `ambiguous`.
- **Bounded.** The walk stops at 8 levels, and `UNION` drops repeated rows, so an inheritance cycle ends.
- **Guards first.** The two cheap `EXISTS` checks run before the walk, so the walk runs only for a candidate whose parent is some type's supertype.
- **The qualifier type carries the namespace.** A `target_module` (PHP pins `$this->m()` to the class's namespace) is satisfied by an inherited candidate when a top-level type named like the qualifier is declared in a file that declares that module. The ancestor itself, often a trait or base class of another namespace, does not have to be.
- **No reparse.** The view reads only columns the index already has, so stored rows are unchanged and `files.content_hash` is kept.

## Verification
- `schema::tests::inherited_member_migration_reaches_only_a_recorded_ancestor` upgrades a version-17 database holding data. An inherited method gains its candidate. The type's own method shadows the supertype's. A type with no recorded ancestor and another language's supertype give none. No file is queued for reparse.
- `crates/mct-index/tests/relation_resolution.rs` runs the rules through a full reindex: a direct supertype, the type's own method, a grandparent's method (resolved), a nearer ancestor's override (it wins over the grandparent's), a cycle, and the negative cases.
- `crates/mct-lang-php/tests/inherited_members.rs`: `$this->log()` and `$this->save()` reach a trait and a parent class of another namespace, and `$this->missing()` stays unresolved.
- This repository (`init --force` on a fresh index, a read-only copy compared with one built from `main`):
  - 48 relations go from `unresolved` to `resolved` (C# 20, PHP 22, Java 4, TS 1). Each goes to the declaring ancestor's method, for example `InMemoryRepository::where`, `Loggable::log` and `Jdbc.findAll`. 6 of them are two or three levels up: `Repository.getById` and `IReadRepository.GetAsync`.
  - 1 goes from `unresolved` to `ambiguous`: `LendingService` reaches `log` through `Auditable`, which uses two traits that both declare it and settles the conflict with `insteadof`, which is not modelled.
  - Nothing else changes state, and dead code is unchanged.
  - Evaluating the view over every relation (Python on the copies, 2 runs each) takes 16.9–17.4 s on `main` and 19.8–19.9 s here. A walk without the guards was about five times slower than `main` in an earlier run.

## Alternatives rejected
- **Resolve inherited members in each parser.** A parser sees one file, and the supertype is often declared in another.
- **Any symbol named like the method whose parent is any type.** That is a match by name, and it is ruled out.
- **One hop only.** It leaves a grandparent's method unresolved: 6 relations on this repository.
- **A walk without guards.** It is correct, but it multiplied the full view evaluation by five.

## Relation to ADR-003 D7
D7 forbids schema migrations in the web/3D phases. This is a separate change with its own issue, and `crates/mct-index/tests/assets.rs` pins the schema version at 18.
