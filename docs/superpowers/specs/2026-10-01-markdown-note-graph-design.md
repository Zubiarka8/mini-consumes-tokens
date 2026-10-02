# Design: Markdown and Obsidian notes as first-class graph nodes

Date: 2026-10-01
Status: implemented for issue #98 (see "Implementation notes" for where it
differs from the proposal)

## Goal

Represent every indexed Markdown document as a searchable note in the symbol
graph, whether or not it has a title or headings. Preserve the relationship
between a note and its sections so links, backlinks, project summaries, and
context retrieval can identify the intended document and return useful
content without guessing from duplicate names.

This design depends on and follows
[`2026-10-01-qualified-relation-identity-design.md`](2026-10-01-qualified-relation-identity-design.md)
(issue #97). In particular, resolved relations point to one snapshot-local
symbol ID; paths and qualified identities are inspectable; ambiguity is
explicit; and no name-only fallback may create an edge. The Markdown-specific
note identity contract below is the one in #97, not a competing identity
scheme.

## Current gaps

- `mct-lang-md` emits ATX heading symbols but no symbol for the file itself.
- Links and tags before the first heading, and all content in heading-less
  notes, have no source symbol and are dropped.
- Setext headings and frontmatter metadata are not represented consistently.
- A note and a heading may have the same name; a basename can also identify
  multiple notes in different folders.
- A heading's current location describes its heading line, not the section
  body needed by context retrieval.
- Resolving `[[Note#Heading]]` by heading name alone can select a same-named
  heading in a different note.

## Identity and graph shape

1. Emit one `SymbolKind::Module` note symbol for every successfully parsed
   Markdown file, including an empty file or one with no heading. Its
   canonical qualified identity is `(language=markdown, normalized
   vault-relative path, kind=module)`, as specified in #97. The database
   symbol ID is the endpoint for graph operations within that index snapshot;
   it is not a durable identifier across reindexing.
2. Normalize paths lexically to `/` separators and a repository-relative
   form. Apply the repository's shared path/case policy from #97. If
   normalization would collapse two distinct indexed paths, report the
   collision as ambiguous; do not silently choose one. The note's path, not
   its title, basename, or alias, is its identity.
3. Store a human-readable note name separately from identity. Prefer a
   non-empty frontmatter `title`; otherwise use a display form of the path
   stem. Keep the qualified path in results so identical titles remain
   distinguishable. Basenames and aliases are lookup terms only. If a lookup
   finds more than one note, return `ambiguous` with candidate identities.
4. Emit ATX and Setext headings as `Element` symbols scoped to their
   containing note. Their qualified identity includes the note path and
   heading's parent chain/name/location. A heading's parent is the note
   module for a top-level heading, or its enclosing heading for a nested
   section. Duplicate heading names within one note remain distinct by their
   qualified parent/location; name alone is not an identity.
5. A note with no headings still has a note symbol and can own references
   found in its body. Content before the first heading belongs to the note
   symbol. Headings and body blocks must never be attached to an unrelated
   note because their display names happen to match.

The path normalization and matching rules must be implemented once in the
shared index/resolver layer so parser, CLI, MCP, and all operating systems
agree. Link matching may accept the documented Obsidian path spellings, but
must not alter canonical note identity.

## Headings, sections, and retrieval

Parse both ATX (`#`) and Setext (`===` / `---`) headings using the Markdown
grammar. Normalize heading text only for anchor matching; preserve source
text for display and search. Where Obsidian-style heading slugs are used,
define one deterministic slug function and retain duplicate slug occurrences
as multiple candidates rather than selecting the first.

Each heading symbol's source range covers the heading and its body through
the line before the next heading of equal or shallower level. A nested
heading's content belongs to that nested section and is excluded from the
parent's direct section range; context assembly may include descendants when
the caller requests them. The note symbol covers the full file. These ranges
allow context tools to retrieve a selected section's text without reading
the entire note. Byte and line boundaries must be consistent with the
existing `Location` contract, including final lines without a trailing
newline.

Use the existing context and summary tools to expose the note and section
symbols. Project summaries should include notes even when they have no H1;
context packs for a note should include its metadata and a bounded body
excerpt, while a context pack for a heading should include that section's
content. Do not add a separate retrieval tool unless implementation proves
the existing tool contracts cannot express this behavior; any new or changed
published MCP signature requires its own compatibility review.

## Frontmatter

Recognize an optional YAML frontmatter block only at the beginning of a
document, delimited by `---` lines. The first implementation should support
and document a narrow, safe subset for `title`, `aliases`, and `tags`, in
scalar, inline-list, and simple list forms. Do not evaluate YAML, execute
tags, or add a general YAML dependency solely for these fields. Malformed or
unsupported values must not make otherwise valid Markdown unparseable; keep
the raw frontmatter in the note's source range and omit only the metadata
value that cannot be read safely.

- `title` supplies the note's display/search name, not its identity.
- `aliases` are alternate note lookup/search terms. Resolve an alias only if
  it identifies exactly one note in the index; collisions are ambiguous.
- `tags` are associated with the note symbol and queryable using the current
  `tag:<name>` reference convention, unless #97's finalized relation model
  provides a more structured metadata representation. Do not invent a
  project-wide tag target symbol as a side effect of a note's frontmatter.

The metadata extractor belongs in `mct-lang-md`. Persistence and search
exposure belong in the shared index layer, not in MCP-specific Markdown
branches.

## Wikilinks, embeds, and resolution

Keep raw target spelling as evidence and preserve whether a reference was a
wikilink or an embed (`![[...]]`). Both may continue to surface through the
existing references APIs for compatibility, with an additive machine-readable
form field if clients need to distinguish embeds. Strip the presentation
alias from the resolution target, but retain it only if useful as display
metadata. Normalize a `.md` suffix for lookup, not for raw spelling.

Resolve a note target conservatively:

1. Resolve explicit vault-relative paths and paths relative to the source
   note using the shared canonical path rules. Reject paths that escape the
   vault root.
2. Resolve a unique frontmatter alias or supported basename form only when
   that lookup yields exactly one note. Multiple candidates are ambiguous.
   Do not fall back from an ambiguous path to a unique heading name.
3. For `Note#Heading`, resolve the note first, then resolve the heading only
   inside that note. For `#Heading`, use the source note as the scope. A
   heading match elsewhere in the vault is never a fallback.
4. If no indexed note/heading matches, retain an unresolved relation. Mark a
   relation external only when source evidence proves it targets outside the
   indexed vault, consistent with #97; a missing file alone is not proof.
5. Persist an exact `to_symbol_id` only for a unique resolved target. Store
   ambiguous candidate IDs as evidence, not traversable edges. Keep raw
   `to_name`/spelling and expose resolution status and qualified target
   identities according to #97.

Links and embeds before the first heading or in a heading-less document are
owned by the note symbol. Links within a heading section are owned by that
heading. Backlink queries should be based on resolved target identity so two
notes with the same title or basename do not share backlinks accidentally.

## Reindex, edits, rename, and deletion

The graph is a derived snapshot of repository contents. Reindexing reparses
notes, headings, metadata, and source relations, then resolves them after all
symbols for the snapshot are available. It must update symbol rows, relation
rows, aliases, and ambiguity candidates transactionally so stale edges cannot
survive a source edit.

Incremental reindex must invalidate and recompute relations when the source
note changes, a possible target note changes, an alias changes, or a file is
renamed/deleted. Rename/delete tests should assert that the old path no longer
appears as a target/backlink, updated links resolve to the new path, and
remaining stale links are unresolved or ambiguous rather than attached to a
same-named note. Snapshot-local symbol IDs may change during rebuild; callers
must use qualified identity across snapshots.

## SQLite migration and MCP compatibility

The migration must be additive and follow #97's relation migration policy:
never backfill a legacy relation by selecting the first matching name. Legacy
relations whose target cannot be proven remain unresolved until their source
is reparsed. Invalidate the appropriate content hashes or provide an explicit
reindex path so notes and relations are rebuilt under the new contract.

Persist the note and heading symbols using the common `symbols` model. Add
only the metadata and relation columns/tables required by the agreed shared
contract (for example, alias lookup and relation resolution/candidate
storage); do not add Markdown-only tables that duplicate common graph
concepts. Recompute resolutions only after the full relevant symbol set is
present, and commit symbol/relation/candidate updates atomically.

Existing MCP inputs and `to_name` output remain compatible. Add qualified
target identity, resolution status, candidates, and reference form as
machine-readable additive output fields, following #97. If path-qualified
lookup is required to distinguish same-named notes, add an optional path
filter without changing the meaning of existing unqualified queries. Existing
queries must not silently change from text/name matching to a different
target. Context output should label note/section content and keep unresolved
or ambiguous relations visibly separate from confirmed graph dependencies.

Before implementation, document the migration behavior and MCP compatibility
in the implementation PR, including whether consumers can ignore additive
fields and how the required reindex is triggered. Do not publish a breaking
tool schema or change the `LanguageParser`/`SymbolRelation` contract without
the workspace-wide implementation and compatibility work called for by #97.

## Decisions to finalize before implementation

- Adopt #97's final path and case-normalization policy verbatim, including how
  Unicode normalization and portable path collisions are reported.
- Confirm the display-name fallback for a note without `title` (path stem or
  full relative path); identity remains the normalized relative path either
  way.
- Confirm the supported frontmatter subset and whether tags stay
  `tag:<name>` relations or use a structured metadata field from #97.
- Specify deterministic anchor slug normalization (including punctuation,
  Unicode, and duplicate headings) and exact inclusive/exclusive section
  range semantics against the existing `Location` model.
- Confirm the minimal SQLite representation for note aliases and ambiguous
  candidates, and the user-visible mechanism for triggering the required
  legacy-index reindex.
- Confirm additive MCP result fields and optional path-filter naming without
  changing existing inputs or the meaning of unqualified queries.

## Implementation phases

1. **Shared contract and persistence (#97).** Finalize canonical path
   normalization, structured source targets, resolution statuses, exact
   target IDs/candidates, migrations, and ID-based traversal. Add resolver
   tests before enabling Markdown-specific resolution.
2. **Markdown note parsing.** Emit note modules for every Markdown file;
   parse ATX and Setext headings; calculate note and section ranges; associate
   pre-heading/heading-less content with the note; extract the documented
   frontmatter subset; retain link/embed form and raw target syntax.
3. **Resolution and query integration.** Resolve paths, aliases, and anchors
   within note scope after indexing; update backlinks, search, project
   summaries, and context rendering while preserving existing MCP fields.
4. **Lifecycle and compatibility verification.** Exercise incremental edit,
   rename, delete, and full reindex; verify migration of a legacy index and
   document user-visible rebuild behavior. Review additive MCP outputs with
   existing consumers before release.

## Acceptance and regression cases

- A heading-less Markdown file is searchable and appears in project
  summaries as a note; an empty file still has a note identity.
- A note with no H1, a Setext-only note, and a frontmatter-only title each
  produce one note node; title/alias do not replace path identity.
- ATX and Setext headings are indexed under their note with correct levels,
  parent relationships, and section boundaries. Duplicate heading names do
  not lose their note/parent identity.
- Links/tags before the first heading and inside a heading-less note are
  retained as relations from the note node.
- A context request for a heading returns its section body, bounded at the
  next equal-or-shallower heading; a request for a note returns note-level
  context.
- A path-qualified wikilink resolves to its note; a unique alias resolves to
  that note; duplicate basenames/aliases return ambiguous candidates.
- `[[Note#Heading]]` resolves the heading only in the resolved note even when
  another note contains a heading with the same name. `[[#Heading]]` scopes
  to the current note.
- `![[Note]]` and `[[Note]]` preserve embed/link form and produce backlinks
  to the correct note without losing raw target spelling.
- Relative paths, `./` and `../`, `.md` suffixes, spaces, nested folders,
  path collisions, and attempts to escape the vault root have deterministic
  tests.
- Frontmatter title, aliases, and tags are searchable/queryable. Malformed
  frontmatter does not reject valid Markdown or create spurious metadata.
- Editing links or aliases, renaming a note, deleting a note, and reindexing
  remove stale edges/backlinks and never rebind to an arbitrary same-name
  note. Full and incremental reindex agree.
- Legacy relations are not assigned targets by name-only migration; MCP
  results retain current fields and add machine-readable resolution and
  qualified identity data.

## Out of scope

- Parsing arbitrary YAML/frontmatter schemas or executing metadata.
- Compiler/LSP-like semantic resolution for non-Markdown languages; that is
  covered by #97's broader resolver contract.
- Choosing a target from a duplicate basename, alias, heading, or title by
  insertion order, recency, or fuzzy score.
- Treating missing targets as external without positive evidence.
- A new context-retrieval MCP tool unless existing tools prove insufficient.
- Making snapshot-local database IDs durable across a reindex.

## Implementation notes

What shipped, and where it deliberately departs from the text above:

- **Note symbol name is the file stem**, not the frontmatter title. A link
  resolves by symbol name, so a title-named note would not answer `[[stem]]`.
  `title`, `aliases` and `tags` are queryable as `title:<t>`, `alias:<a>`,
  `tag:<t>` references from the note. **Alias/title lookup of a wikilink
  target is not implemented**: `[[Alias]]` stays `unresolved` (never guessed).
- **Top-level headings keep `parent = None`.** The note path already scopes
  them, and `get_file_skeleton`/`get_project_overview` treat parentless symbols
  as a file's top level. Nested headings are parented by heading level; Setext
  headings are handled by level because `tree-sitter-md` nests sections for ATX
  only.
- **Section ranges** run from the heading to the line before the next heading
  of equal or shallower level and therefore include descendants (a contiguous
  range cannot exclude nested sections). The note spans the whole file.
- **Embeds are `Imports`, links `References`** — no new field or kind.
- **Shared contract (additive):** `RelationTarget` gained `kind` and `module`
  (migration 10: `relations.target_kind/target_module`, `relation_candidates`
  recreated, every file reparsed). `kind` makes a note link target a `module`,
  never a same-named heading; `module` scopes `[[bare#Heading]]` to files whose
  note symbol is `bare`. Path-written links carry an exact `path`. Escaping
  the root keeps the raw spelling and stays unresolved. No Markdown rule lives
  in `mct-index`/`mct-mcp-server`.
- **Existing tools only.** `get_project_overview` now lists notes (module
  symbols); `build_context_pack` of a `module`/`element` also lists
  `references`/`imports` referrers by resolved identity (backlinks), with
  ambiguous links under *Uncertain relations*. `find_references` is unchanged.
- **Limits:** heading match is exact text (no slug/case folding, last `#`
  segment only, `^block` ignored); link paths match case-sensitively, from the
  vault root or `./`/`../` of the source note (no Obsidian shortest-suffix
  paths); only `title`/`aliases`/`tags` frontmatter; inline code and fenced
  blocks are skipped but HTML comments / `%%comments%%` are not.
