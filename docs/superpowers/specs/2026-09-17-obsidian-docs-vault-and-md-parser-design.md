# Design: Obsidian-style docs vault + `ccm-lang-md` Phase 2 (PKM relations)

Date: 2026-09-17
Status: approved, pending implementation plan

## Goal

Two independent but related deliverables:

1. A token-optimized "second brain" documentation vault under `docs/`, structured as atomic, cross-linked notes (Obsidian-compatible).
2. Extend `crates/ccm-lang-md` (currently "Phase 1": ATX headings only, zero relations — see its module doc comment) to index the PKM conventions used by that vault — `[[WikiLinks]]` and `#tags` — into the existing symbol/relation model, with **no changes to the shared `ccm-core` schema**.

## Non-goals

- No new `SymbolKind` or `RelationKind` variant.
- No new field on `SymbolRecord` (ruled out explicitly — see Decision 1).
- No inline-grammar reparse of Markdown (CommonMark inline AST has no concept of `[[...]]` or `#tag`; see Decision 4).
- Links/tags inside table cells are not scanned (a distinct block-grammar node with no `paragraph` child, never visited by the two node kinds this phase scans). Links/tags inside list items, blockquotes, and inline code ARE scanned like ordinary paragraph text — `tree-sitter-md` gives them no distinct node kind of their own at the block-grammar level this crate parses, so their text lands in the same `paragraph`/heading `inline` text already scanned. This is an accepted consequence of the hand-rolled, node-kind-based scanning approach (Decision 4), not a deliberately built feature; excluding them would require detecting the containing block type, deferred to a future phase.
- No per-language doc files beyond the one exception noted in Decision 6.

## Decisions (confirmed with user before this doc was written)

1. **Tags → relations, not a schema field.** `SymbolRecord` has no attribute field today (`id/name/kind/location/parent` only). Adding one would touch every `LanguageParser` impl, the SQLite schema/migrations, and the MCP tool output shape — the project's own `CLAUDE.md` flags this as cross-cutting, requiring a separate issue, not a drive-by change. Instead: a tag emits `RelationKind::References` from the owning heading symbol to `to_name = format!("tag:{tag}")`. Queryable today via `find_references`/`find_calls` with no schema change.
2. **Orphan links/tags are skipped.** A `[[WikiLink]]` or `#tag` appearing before the first heading, or in a heading-less file, has no enclosing symbol to set as `SymbolRelation::from`. Consistent with the crate's existing rule that a heading-less document produces zero symbols — these are silently not indexed.
3. **WikiLink targets are indexed verbatim, alias stripped.** `to_name` = raw text between `[[` and `]]`. `[[Page#Heading]]` is NOT split — indexed literally as `"Page#Heading"` (anchor resolution deferred). A trailing `|alias` display suffix IS stripped (`[[Page|shown text]]` → target `"Page"`), since the alias is display-only, not part of the reference identity.
4. **No new dependency, no inline-grammar reparse.** `tree-sitter-md`'s inline grammar is CommonMark; it has no `[[WikiLink]]`/`#tag` node kinds, so coercing AST nodes out of it would be fragile. Scanning is hand-rolled byte/text scanning over two already-visited node kinds' raw text (`atx_heading` content, `paragraph` blocks) — no `regex` crate (nothing in this workspace depends on `regex` today; two fixed literal patterns don't justify adding one).
5. **Relation location is block-granular.** A relation's `Location` resolves to the whole containing heading/paragraph block's span, not the exact bracket offset. Refining to exact match spans is deferred.
6. **`docs/02-crates/parsers/` holds only `overview.md` (the parser contract every `ccm-lang-*` crate follows) plus `ccm-lang-php.md`** (the one language with a documented security exception, `sec-001-php-stack-overflow`). No per-language file for languages with nothing special to say — `00-index.md`'s extension→crate table covers those.

## Part 1: Docs vault structure

```
docs/
  00-system/
    00-index.md          # MOC: links every note below + extension->crate table
    glossary.md
  01-architecture/
    mcp-protocol-spec.md
    adrs/
      001-sqlite-storage.md
  02-crates/
    core/
      ccm-mcp-server.md
    parsers/
      overview.md         # mandatory contract: LanguageParser trait, AST/tree-sitter
                           # conventions, stack-overflow prevention (MAX_TRAVERSAL_DEPTH)
      ccm-lang-php.md      # exception case only, per Decision 6
  03-performance/
    limits-spec.md
  04-security/
    advisories/
      sec-001-php-stack-overflow.md
  05-backlog/
    roadmap.md
  06-templates/
    adr-template.md
    crate-spec-template.md
    lang-spec-template.md  # includes the explicit rule: only use this template for
                            # a new language note if it has special behaviors/macros;
                            # otherwise just add a row to 00-index.md's table
```

Conventions applied to every note:
- Atomic (one concept per file), concise, bullet-structured.
- Cross-linked via `[[WikiLink]]` to related notes (e.g. `sec-001-php-stack-overflow.md` links `[[ccm-lang-php]]` and `[[overview]]`).
- Tagged with `#tag` where it aids retrieval (e.g. `#security`, `#adr`, `#performance`).
- Templates capped at ~100 words including their own rules-for-use text.

`.obsidian/` added to `.gitignore` (vault-local Obsidian workspace state, not repo content).

## Part 2: `ccm-lang-md` Phase 2

### Threading change

`Walker::visit`/`visit_children` currently pass `parent_name: Option<String>` through the recursion. Add `parent_id: Option<SymbolId>` alongside it, populated from `push_symbol`'s return value when descending into a heading's section. Both a heading's own text and paragraph blocks inside its section scan relative to that heading's id (a heading's own inline tags/links attach to the heading itself; a paragraph's attach to its nearest enclosing heading).

### Scanning

New module-private function, e.g. `scan_relations(text: &str) -> Vec<(RelationKind, String)>` (or emits directly via a closure/`&mut Vec<SymbolRelation>` — implementation detail for the plan), applied to:
- `heading_text(heading, source)` output (already isolates content past the ATX `#` markers, so no false-positive risk from the heading syntax itself).
- Each `paragraph`-kind child node's raw text within a section, scanned before recursing into nested subsections.

Pattern 1 — WikiLink: `[[` ... `]]`, non-greedy, target = content up to a `|` if present else up to `]]`, trimmed. Emits `RelationKind::References` with that target as `to_name`.

Pattern 2 — Tag: `#` immediately followed by `[A-Za-z0-9_/-]+`, required to be preceded by start-of-text or whitespace (excludes URL fragments like `.../#section` and heading-marker false positives). Emits `RelationKind::References` with `to_name = format!("tag:{captured}")`.

Known false-positive risk, accepted and documented in the module doc comment rather than solved: a tag-shaped token inside inline code (`` `#notatag` ``) is still matched, since code-span exclusion needs the inline grammar (out of scope per Decision 4).

### Symbol kind

Unchanged — headings remain `SymbolKind::Element`, as today.

## Testing

`crates/ccm-lang-md/tests/parse.rs`:
- Wikilink inside a paragraph → `References` relation, correct `from` (enclosing heading id) and `to_name`.
- `[[Page|alias]]` → target is `"Page"`, alias dropped.
- `[[Page#Heading]]` → target is `"Page#Heading"` verbatim.
- `#tag` in a heading's own text → relation from that heading.
- `#tag` in a paragraph → relation from the enclosing heading.
- Link/tag appearing before any heading → no relation emitted, no panic.
- URL fragment (`http://x/#frag`) in a paragraph → not matched as a tag.
- Existing `no_relations_are_ever_emitted_in_phase_1` test retired/renamed — no longer true; replace with an equivalent "Phase 1 heading-only behavior is preserved" assertion if still valuable, or fold into the above.

`crates/ccm-lang-md/tests/index_integration.rs`: extend or add a fixture exercising a wikilink + tag through the real SQLite index (`find_references`-style query), matching the existing fixture style in `tests/fixtures/docs-project/`.

## Validation

- `cargo check --workspace`
- `cargo test -p ccm-lang-md`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- Manual check: `docs/02-crates/parsers/` contains exactly `overview.md` and `ccm-lang-php.md`, nothing else.

## Open follow-ups (explicitly out of scope for this spec, noted for the backlog)

- Anchor-aware resolution of `[[Page#Heading]]` against the target file's actual heading symbol.
- Tags/links inside list items, blockquotes, tables.
- Code-span exclusion (needs inline-grammar reparse).
- Exact-span relation locations instead of block-granular.
