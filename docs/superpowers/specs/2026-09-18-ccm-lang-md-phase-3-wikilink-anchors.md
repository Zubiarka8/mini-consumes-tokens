# Design: `ccm-lang-md` Phase 3 (wikilink anchors, `.md` normalization, embeds)

Date: 2026-09-18
Status: approved, implemented

## Goal

Extend Phase 2's `[[WikiLink]]` scanning (see
`2026-09-17-obsidian-docs-vault-and-md-parser-design.md`) to resolve
`[[Note#Heading]]` anchors against real symbols instead of indexing them as
one verbatim string, normalize a `.md` suffix so `[[Note]]` and `[[Note.md]]`
are equivalent, and lock in `![[Embed]]` support — with no changes to the
shared `ccm-core`/`ccm-index` schema.

## Non-goals

- No new `SymbolKind` or `RelationKind` variant (same non-goal as Phase 2).
- No new field on `SymbolRecord`/`SymbolRelation`.
- No `ccm-core`, `ccm-index`, or `ccm-mcp-server` changes.
- No fix for whole-note resolution depending on the "a note's own heading is
  named like the note title" convention — this crate still has no
  per-file/document symbol. Explicitly considered and rejected: adding a
  `SymbolKind::Document` would make this more correct for arbitrary Obsidian
  vaults, but touches `ccm-core` and reverses Phase 2's approved
  no-new-`SymbolKind` decision. Confirmed with the user to keep this
  accepted, documented limitation rather than widen scope.
- No inline-grammar reparse, no new dependency (same as Phase 2 — Decision 4
  in the prior spec still applies).

## Decisions

1. **Two relations, not one composite target.** `SymbolRelation` has a
   single `to_name: String` and this codebase has no path-qualified relation
   concept anywhere (every relation, in every language, resolves one
   unscoped name). `[[Note#Heading]]`'s alias-stripped identity is split on
   the first `#`; each non-empty part (note, heading) becomes its own
   `RelationKind::References` relation from the same source symbol/location.
   `[[#Heading]]` (empty note part) naturally emits only the heading
   relation — a side effect of "only emit non-empty parts," not a
   deliberately built feature.
2. **`.md`/`.MD` suffix stripped from the note part only**, case-insensitive,
   via `str::get`-guarded slicing (never a raw byte-offset slice, which could
   panic on non-ASCII input landing mid-character).
3. **Embeds need no code change.** `![[Note]]`'s leading `!` sits outside the
   `[[...]]` span `scan_wikilinks` matches, so it was already scanned
   identically to a plain wikilink since Phase 2 — this phase adds tests and
   documentation, not new scanning logic.
4. **Malformed nested-bracket fix.** A candidate span that itself contains a
   nested `[[` is rejected and the scan resumes from just past the *outer*
   `[[` (not past the wrongly-matched `]]`), so a well-formed inner
   `[[Real]]` is still found instead of being swallowed into garbage text.
   This is a correctness fix uncovered while implementing this phase, not a
   design trade-off.
5. **Whole-note resolution is unchanged.** A `[[NoteTitle]]` link still only
   resolves to a real symbol when some heading (typically the target note's
   own H1) is literally named `NoteTitle` — see Non-goals.

## Testing

`crates/ccm-lang-md/tests/parse.rs`: anchor splits into two relations (alias
present or not); same-document `[[#Heading]]` emits only the heading
relation; `.md`/`.MD` suffix normalized away; `![[Embed]]` and
`![[Embed#Heading]]` behave identically to non-embed forms; multiple distinct
wikilinks and a repeated wikilink (not deduped); the nested/malformed-bracket
case still finds the well-formed inner link; unbalanced single brackets are
not mistaken for a wikilink.

`crates/ccm-lang-md/tests/index_integration.rs` + `fixtures/docs-project/architecture.md`:
a `[[Parser#Testing]]` link's heading half (`"Testing"`) is discoverable via
`find_references` through the real SQLite pipeline; a link to a genuinely
nonexistent target (`[[Nonexistent Page]]`) is still discoverable by name
(matches pre-existing `ccm-index` behavior — a relation is stored regardless
of whether it resolves).

## Validation

- `cargo test -p ccm-lang-md`
- `cargo fmt`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`

## Open follow-ups

- Whole-note resolution independent of the heading-naming convention (would
  need a `SymbolKind::Document`-style change to `ccm-core`) — explicitly
  deferred, see Non-goals.
- Tags/links inside table cells (still unscanned, same as Phase 2).
- Code-span exclusion (needs an inline-grammar reparse, same as Phase 2).
- Exact-span relation locations instead of block-granular (same as Phase 2).
