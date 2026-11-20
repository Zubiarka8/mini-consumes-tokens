# ADR-040: A nested function is a candidate only from its own file

## Status
Proposed (2026-11-20). Issue: [#376](https://github.com/Zubiarka8/mini-consumes-tokens/issues/376). Adds schema migration 23, on top of migration 22 ([ADR-039](039-supertype-type-candidates.md), PR #597). Amends the candidate rules of [ADR-002](002-qualified-relation-resolution.md). Extends [ADR-035](035-function-local-variable-scope.md) from variables to functions. Keeps the clauses of [ADR-026](026-same-file-module-shadowing.md), [ADR-036](036-not-field-relation-target.md) and [ADR-037](037-inherited-member-candidates.md).

## Context
- A function declared inside another function's body is reachable by name only from that body. No supported language lets another file call it by name.
- `relation_candidates` matched it from any file. A lone match was reported `resolved`: `matchMedia` in `web-ui/src/hooks.ts` resolved to a `const matchMedia = …` inside the setup function of the dashboard test harness.
- ADR-035 scopes variables by `parent`. That does not carry over: JS/TS, Python, Kotlin, C#, Lua and PHP give a nested function no `parent`. Only Rust and Bash set one.

## Decision
Migration 23 recreates `relation_candidates` with one more predicate. Every other predicate is unchanged.

```sql
AND (sf.id = cf.id
     OR s.kind <> 'function'
     OR s.end_line IS NULL
     OR NOT EXISTS (
         SELECT 1 FROM symbols p
         WHERE p.file_id = s.file_id
           AND p.id <> s.id
           AND p.kind IN ('function', 'method')
           AND (p.line < s.line
                OR (p.line = s.line AND p.column < s.column))
           AND (p.end_line > s.end_line
                OR (p.line = s.line AND p.end_line = s.line
                    AND s.end_line = s.line
                    AND p.column + p.byte_len >= s.column + s.byte_len))))
```

- **Containment by span.** The index stores a symbol's start line and column, its byte length and its end line, but no end column. A function or method of the same file encloses the candidate when it starts first and ends on a later line. When both lie on one line (a minified bundle), the byte lengths decide.
- **Conservative where the span cannot tell.** An encloser that starts on an earlier line and ends on the candidate's end line could be a sibling (`} function b() {}`), so the candidate stays. A symbol without `end_line` stays.
- **Only `function` candidates.** A method belongs to a type and is reached through it. Variables already follow ADR-035.
- **Same file unchanged.** A relation from the function's own file keeps it.
- **No reparse.** The view reads only existing columns, so stored rows are unchanged and `files.content_hash` is kept.

## Verification
- `schema::tests::nested_function_migration_scopes_candidates_to_their_file` upgrades a version-22 database holding data. It checks a nested function seen from another file and from its own file, a nested and a sibling function on one minified line, a top-level function, the same-end-line case and a symbol without `end_line`. No file is queued for reparse.
- Cost: a full scan of the view on this repository's index went from 26.0 s (275,234 rows) to 27.3 s (265,980 rows). The subquery runs only for a `function` candidate from another file, through `idx_symbols_file`.
- On this repository (`mct-cli init`, view 22 vs view 23 on a copy of the index): 2,923 relations change their candidates. 118 go from `ambiguous` to `resolved`, 112 from `resolved` to `unresolved`, 14 from `ambiguous` to `unresolved`, and 2,679 stay `ambiguous` with fewer candidates. By caller language: 1,662 Rust, 1,257 JS/TS, 3 Python, 1 Kotlin. A sample of the dropped candidates showed only functions inside another file's function: the `harness.mjs` fixture (`matchMedia`, `addEventListener`), the `Constellation` component's `move`, and helpers nested in Rust tests.

## Alternatives rejected
- **A stored `local` flag filled by each parser.** Cheaper to query, but it changes `LanguageParser` output in every parser and needs a full reparse. Issue #446 (file-scoped symbols) may still add a flag. This view rule does not block it.
- **Use `parent`.** Most parsers leave it empty for a nested function.
