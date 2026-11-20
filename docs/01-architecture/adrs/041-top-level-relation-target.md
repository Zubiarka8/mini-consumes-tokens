# ADR-041: A JS/TS name with no receiver has no type-member candidates

## Status
Proposed (2026-11-20). Issue: [#395](https://github.com/Zubiarka8/mini-consumes-tokens/issues/395). Adds schema migration 24, on top of migration 23 ([ADR-040](040-nested-function-scope.md), PR #600). Amends the candidate rules of [ADR-002](002-qualified-relation-resolution.md). Extends [ADR-036](036-not-field-relation-target.md) (`not_field`) from fields to methods.

## Context
- In JS/TS a name the source writes with no receiver (`f()`, `new Repo()`, `<Button />`, a type `Paid`, `import { f }`, `x instanceof Marker`) names a binding in scope, never a member of a class, an interface or an enum. JavaScript has no implicit `this`.
- `relation_candidates` matched by name, so a same-named method of any type was a candidate as well. Bare calls and type names already carry `not_field` (ADR-036), which rules out fields but not methods: `format()` was ambiguous with `Money.format`.
- The issue proposed `top_level = 0 OR s.parent IS NULL`. That would break same-file calls to a nested arrow function: the JS/TS parser gives `const f = () => …` inside a function its enclosing function as `parent`.

## Decision
`RelationTarget` gains `top_level: bool`, stored as `relations.top_level`. Migration 24 adds the column (default 0), recreates `relation_candidates` with one more predicate and empties `files.content_hash` so every file is reparsed. Every other predicate is unchanged.

```sql
AND (r.top_level = 0 OR s.kind NOT IN ('method', 'field'))
```

- **By kind, not by `parent`.** A method or field is a type member in every language. A nested function, a namespace function or an object-literal function (`kind = 'function'`) stays a candidate.
- **The JS/TS parser sets it** on a bare identifier call, a `new` of a bare identifier, a bare JSX component tag, a type identifier not inside a `nested_type_identifier` (`Paid`, not the `Open` of `State.Open`), a bare `instanceof` operand, an import binding, and a same-file value use. A member access (`o.g()`, `new ns.Repo()`, `<UI.Card />`) leaves it unset.
- **Other languages leave it unset.** Java's bare `m()` names a member of the enclosing class, for example.

## Verification
- `schema::tests::top_level_migration_drops_type_members_of_bare_names`: an existing row keeps its candidates, a flagged one keeps only the function, a member access keeps the members, and every file is queued for reparse.
- `mct-lang-js-ts/tests/top_level_names.rs`: a class method, a class field, an interface method and an enum member each share a name with a top-level binding of another file. Each bare use resolves to the binding only (it failed without the predicate). `m.format()` still sees the method. A parse test checks which syntax sets the flag.
- On this repository (`mct-cli init`, migration 23 vs 24, 24,786 JS/TS relations): 100 ambiguous relations lose candidates and 14 go from `ambiguous` to `resolved` (for example `close()` in `web/app/core.js`, `add()` in the webapp fixture). No relation loses a resolution.

## Alternatives rejected
- **`qualifier IS NULL` in the view.** `Money.ofMinor(0, c)` is also stored with no qualifier and `member = 0`, so static calls would break.
- **`s.parent IS NULL`** (the issue's first draft). See Context.
