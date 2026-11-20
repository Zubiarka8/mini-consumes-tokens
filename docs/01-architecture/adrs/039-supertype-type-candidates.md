# ADR-039: A C# or C++ supertype relation has only type candidates

## Status
Proposed (2026-11-20). Issue: [#388](https://github.com/Zubiarka8/mini-consumes-tokens/issues/388). Adds schema migration 22, which [ADR-003](003-web-3d-indexing.md) D7 does not cover. Amends the candidate rules of [ADR-002](002-qualified-relation-resolution.md). Keeps the clauses of [ADR-026](026-same-file-module-shadowing.md), [ADR-035](035-function-local-variable-scope.md), [ADR-036](036-not-field-relation-target.md) and [ADR-037](037-inherited-member-candidates.md).

## Context
- An `extends`/`implements` relation names a type. In C# and C++ a constructor is spelled like its class and is indexed as a `method` of it, so `relation_candidates` also offered every constructor: `struct PerishableProduct : Product` was `ambiguous` between the class, its constructor and a same-named function.
- Java fixed this in its parser (#387): its supertype relations carry `target_kind`. C# and C++ cannot always tell a class base from an interface base from syntax (`class A : B, IC`), so one parser-side kind does not fit.
- Other languages can extend a non-type value: JS `class A extends Base` where `Base` is a constructor function, Python `class User(Base)` where `Base = declarative_base()`. A rule for every language would lose those edges.

## Decision
Migration 22 recreates `relation_candidates` with one more predicate. Every other predicate is unchanged.

```sql
AND NOT (r.kind IN ('extends', 'implements')
         AND sf.language IN ('csharp', 'cpp')
         AND s.kind IN ('method', 'function', 'field', 'variable', 'constant'))
```

- **Only C# and C++.** In those languages a base list names a type; a value is never a base.
- **Only supertype relations.** A `calls` to a constructor (`new Repo()`, `: base(…)`) keeps its method candidates.
- **No reparse.** The view reads only columns the index already has, so stored rows are unchanged and `files.content_hash` is kept.

## Verification
- `schema::tests::supertype_migration_drops_non_type_candidates_in_csharp_and_cpp` upgrades a version-21 database holding data: the C# and C++ `extends` keep only the class, a JS class extending a function keeps it, and a call keeps the constructor. No file is queued for reparse.
- On this repository, `mct-cli init` with `origin/main` (`37d4920a`) and with this change, counting every C# and C++ `extends`/`implements` hit to a class, struct or interface declared in those parser crates (`find_references`): C++ goes from 4 `ambiguous` and 1 `resolved` to 5 `resolved` (`PerishableProduct : Product` had `Product`'s constructor and a same-named function as candidates, `NotFound`/`Conflict : StorageError` the constructor). C# is 30 `resolved` before and after: its parser already pins these targets, so the C# clause only guards relations that carry no kind.
