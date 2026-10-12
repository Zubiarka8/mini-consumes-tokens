# ADR-005: Markdown frontmatter alias storage

## Status
Accepted (2026-10-12). Issue: [#135](https://github.com/Zubiarka8/mini-consumes-tokens/issues/135). Adds schema migration 11, which [ADR-003](003-web-3d-indexing.md) D7 does not cover (see "Relation to ADR-003 D7"). Builds on [ADR-002](002-qualified-relation-resolution.md) (`relation_candidates` resolution).

## Context
- The Markdown note graph (#98) names a note's `module` symbol after the file stem. `[[Alpha]]` stays `unresolved` even when a note declares `aliases: [Alpha]`. The note-graph spec requires that a unique alias resolve to its note.
- The parser already emits `alias:<a>` references from the note. A reference is evidence that a note mentions a name, not a lookup key for other files' relations. The `relation_candidates` view matches `symbols.name = relations.to_name`, so it cannot reach a note through those references.
- Resolution is computed on read. The candidate count decides the state: one candidate is `resolved`, several are `ambiguous`, none is `unresolved`. Nothing chooses among candidates, so an alias index stored per file stays correct across incremental reindexes. A referrer that has not changed is not reparsed, and its links are re-resolved anyway.
- The schema had 10 migrations. `crates/mct-index/tests/assets.rs` asserts `user_version == 10` and allows a bump only with a new ADR.

## Decision

### Storage
Migration 11 adds:

```sql
CREATE TABLE symbol_aliases (
    symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    PRIMARY KEY (symbol_id, alias)
);
CREATE INDEX idx_symbol_aliases_alias ON symbol_aliases(alias);
```

- `mct-core`: `ParsedFile::symbol_aliases: Vec<SymbolAlias { symbol, name }>`. This is an additive field. A parser that does not set it leaves it empty (`ParsedFile` derives `Default`). An alias names a symbol of the same parsed file. The indexer drops an alias whose symbol is unknown or whose text is empty.
- `mct-lang-md` declares one alias for each frontmatter `aliases` entry, on the note's `module` symbol. A `title` is never an alias.
- The rows belong to the symbol, so a reparse, rename or deletion of the declaring file replaces or removes them through the cascade.

### Resolution
Migration 11 recreates `relation_candidates` with two changes. Every other predicate is unchanged.
1. A relation with `target_kind = 'module'` and no `target_path` (a bare `[[Name]]` or `![[Name]]`) also matches symbols that declare `to_name` as an alias. Path links (`[[a/Name]]`) never use aliases.
2. A `target_module` scope (`[[Name#Section]]`) also accepts a note that declares `Name` as an alias.

Rules that follow from this:
- **Comparison is exact and case-sensitive**, like stems and headings. `[[alpha]]` does not match `aliases: [Alpha]`. Case folding is out of scope (issue #135) and would have to change stems and headings in the same release.
- **Ambiguity keeps every target.** An alias declared by several notes, or one note's alias equal to another note's stem, gives `ambiguous` with every matching note. `[[Alias#Section]]` keeps both of its edges. The note edge stays `ambiguous` with every note. The section edge lists the matching section of every candidate note. It is the same rule the duplicate-basename case already follows. Nothing is chosen by order, recency or score.
- **No title fallback.** A title-only match stays `unresolved`.

### Migration
The table starts empty. Legacy relations are not backfilled by name (#97 policy): an `alias:<a>` reference or a matching name does not prove that a note declares the alias. The migration sets every `content_hash` to `''`, so the next normal reindex reparses every file and fills the table. No `reindex --force` is needed.

### Relation to ADR-003 D7
ADR-003 D7 states "No schema migration in any phase" for the seven web/3D phases of that plan, and requires a new ADR for any future schema need. This ADR is that new ADR, for a Markdown feature outside the web/3D plan.
- D7 still holds for its phases: none of them adds a migration, and assets and findings remain `files`/`symbols` rows.
- D7's consequence "an older binary opens a newer index" no longer holds from migration 11 on. A binary built before this change stops on a migrated index with `CONNECTION_CLOSED` ("migration number that is too high"). The fix is the one already documented: `scripts/unix/reinstall.sh --reindex` (Windows: `scripts\windows\reinstall.ps1 --reindex`).
- `assets.rs` keeps its gate and now expects 11, pointing at this ADR. A later migration needs another ADR.
- ADR-003 is not edited. Its rule was scoped to its own plan and was not wrong when written.

## Alternatives considered

| Option | Verdict |
|---|---|
| (a) `symbol_aliases` table plus a widened view (migration 11) | **Chosen.** One indexed lookup. Aliases have their own lifecycle through the cascade. No duplicate symbols. |
| (b) No migration: one extra `module` symbol per alias in the declaring file | Rejected. Every alias would show up as a separate symbol in `list_symbols`, `find_symbol`, search, skeletons and overviews. The view would still need a rule to tell an alias symbol from a real note. |
| (c) No migration: resolve through the existing `alias:<a>` references | Rejected. It would need the view recreated (itself a migration), or resolution moved into Rust outside the view. Tools that read `relation_candidates` would then disagree with each other. |
| (d) An `aliases` column on `symbols` (JSON or delimited) | Rejected. Matching would need `json_each` or string search per candidate row, with no index on the alias. |

## Consequences
- Wikilinks resolve by frontmatter alias, and every ambiguous outcome lists all its candidates. Tests: `crates/mct-lang-md/tests/note_graph.rs` covers unique, shared, stem-colliding, case and title cases, anchors through ambiguous names, and incremental versus full reindex. `crates/mct-index/src/schema.rs` tests that the migration starts empty and invalidates hashes once.
- Query cost: the view keeps one `SELECT`, with a correlated `IN (… UNION ALL …)` lookup. It is measured with `crates/mct-index/tests/relation_scale.rs`, as recorded in the task checkpoint.
- Older binaries need a reinstall and reindex after switching to a branch with migration 11.
- Other parsers can declare aliases later through the same field. The view uses them only for `module` targets without a path, so the change does not affect other languages until a parser opts in.
