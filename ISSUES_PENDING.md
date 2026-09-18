# Pending issues

Product bugs found by the test-coverage audit (2026-09-18) and deliberately
left **unfixed**. Each one is pinned by an `#[ignore]`d test that documents the
observed-versus-expected behavior; the ignored tests are the specification for
the fix, so a fix is done when its test passes with the `#[ignore]` removed.

Run them with `cargo test --workspace -- --ignored`.

---

## 1. `get_file_skeleton` / `get_project_overview` are blind to 4 of 16 languages

**Affected**: Go, C#, Bash, PowerShell.

**Observed**
- `get_file_skeleton` returns `No top-level symbols found in <path> to build a
  skeleton from.` for every file in those languages.
- `get_project_overview` lists the module but only ever renders the synthetic
  whole-file `[module]` stub under it — no real declarations.

**Expected**: the file's declarations, exactly as the tool's own description
promises ("Brace-delimited languages (Rust, Go, Java, C++, C#, PHP, JS/TS,
Kotlin) get precise body elision; other languages (e.g. Python, Lua, Bash,
PowerShell) get a best-effort declaration-line-only rendering"). Go and C# are
named in that description.

**Root cause**: both tools define "top-level declaration" as
`parent.is_none()` —
- `get_file_skeleton`: `entry.parent.is_none() && entry.kind != "module"`
- `get_project_overview`: `e.parent.is_none() && OVERVIEW_KIND_ALLOWLIST.contains(&e.kind)`

For these four parsers a top-level function *does* carry a parent — the Go
package (`backend`), the C# namespace (`Omni.Payments`), or the synthetic
file-module (`lib`, `Common`). Every real declaration is therefore filtered
out, and the `module` entries that survive are dropped by the same filter.

**Fix direction**: stop using `parent.is_none()` as the definition of
top-level. Treat "parent is the file's own module/package/namespace symbol" as
top-level too, or record a depth/level on the symbol at parse time.

**Pinned by** (`crates/mct-mcp-server/tests/omni_tools.rs`):
- `get_file_skeleton_renders_declarations_for_every_registered_language` *(ignored)*
- `get_project_overview_surfaces_real_declarations_for_every_registered_language` *(ignored)*
- `get_file_skeleton_is_currently_empty_wherever_declarations_carry_a_parent` — passing; pins today's behavior so the blast radius cannot silently reach a fifth language
- `get_project_overview_currently_degrades_to_module_stubs_for_parented_languages` — passing; same purpose

---

## 2. `.lua` files are dropped silently, with no diagnostic at all

**Observed**: `crates/mct-lang-lua` implements `LanguageParser`, but it is not
wired into `build_registry()` (it is the plugin-architecture proof, not a
shipped language). `.lua` is *also* absent from `KNOWN_PENDING_LANGUAGES`, so a
`.lua` file in an indexed repo produces:
- no `files` row,
- no symbols,
- no `unsupported_languages` entry,
- no reported issue.

It vanishes without a trace. A user with Lua in their repo gets no signal that
part of their codebase is invisible to every query.

**Expected**: whichever is intended — either register the parser, or list
`lua` in `KNOWN_PENDING_LANGUAGES` so `get_indexing_status` reports it as a
known-unsupported extension. Silence is the one wrong answer.

**Decision needed before fixing**: is Lua meant to ship? That answer picks the
fix. See `internal/checklist.md`.

**Pinned by** (`crates/mct-mcp-server/tests/omni_fixture.rs`):
- `a_lua_file_is_skipped_without_any_diagnostic_because_lua_is_not_registered` — passing; pins the hole rather than blessing it

---

## 3. Obsidian-vault Markdown: seven gaps in the note/link graph

All pinned in `crates/mct-lang-md/tests/index_integration.rs` against the
synthetic vault at `crates/mct-lang-md/tests/fixtures/vault/`. Each is a real,
reproducible loss of edges or resolution on vault layouts that are routine in
practice.

| # | Gap | Observed | Expected |
|---|-----|----------|----------|
| 3.1 | Heading level is not stored | `# H1` … `###### H6` all index as `kind == "element"`; neither `SymbolHit` nor `SymbolListEntry` carries a level. Parent nesting is the only hierarchy signal left, and it cannot be inverted into a level because documents skip levels (`##` then `####`). | An H1 and an H6 are distinguishable in the index. |
| 3.2 | Embeds collapse into links | `![[Glossary]]` and `[[Glossary]]` both yield `RelationKind::References` → `"Glossary"`. The leading `!` sits outside the span `scan_wikilinks` matches and is never recorded. | A distinct relation kind (or flag) for embeds — an embed changes rendered content, a link does not. |
| 3.3 | YAML front-matter is skipped entirely | `daily/2026-09-18.md` declares `tags: [daily, review]`; the file parses cleanly but `find_references("tag:daily")` returns nothing. The parser scans only `atx_heading` text and `paragraph` nodes. Inline `#standup` *is* indexed. | Front-matter tags indexed like inline ones — front-matter tagging is the Obsidian template default, so such vaults get no tag graph at all. |
| 3.4 | Path-form wikilinks are not normalized | `[[notes/Glossary]]` is stored verbatim as `to_name = "notes/Glossary"`, matching no symbol; only `strip_md_extension` runs, never a path-tail split. Obsidian treats it as the same note as `[[Glossary]]`. | Resolves to the same target as a bare `[[Glossary]]`. |
| 3.5 | Links in heading-less notes are dropped | `orphan.md` has no heading → zero symbols. `push_relations_from_text` needs an enclosing heading symbol to hang a relation off, so a paragraph with `parent_id == None` is skipped; its `[[Glossary]]` link and `#orphan` tag vanish with no reported issue. | A file-level symbol (or equivalent) anchors such links — heading-less capture notes are routine. |
| 3.6 | A note is addressable only by its H1 | `notes/Untitled Capture.md` starts with `# A Different Title`, so its only symbol is `"A Different Title"`. `[[Untitled Capture]]` is recorded but matches nothing. Whole-note resolution relies entirely on the convention that H1 == file name. | `[[Untitled Capture]]` reaches `notes/Untitled Capture.md`. Every renamed or untitled note breaks the convention. |
| 3.7 | Anchors lose their note scope | `[[Project Alpha#Goals]]` splits into two independent relations, `→ "Project Alpha"` and `→ "Goals"`, with nothing tying them together. The fixture vault deliberately holds two `## Goals` headings, so the heading edge is ambiguous by construction; querying the composite name returns nothing. | The anchor resolves to exactly one heading — the one in the note the link named. |

**Common thread for 3.4–3.7**: `mct-lang-md` has no file/note-level symbol and
no path-aware link resolution. A single fix introducing a per-file note symbol
and resolving wikilinks against file names (not just H1 text) would close 3.4,
3.5 and 3.6 together, and gives 3.7 the scope it needs to disambiguate.

**Pinned by** *(all ignored)*: `a_heading_records_the_level_it_was_written_at`,
`an_embed_is_distinguishable_from_a_plain_wikilink`,
`front_matter_tags_are_indexed`,
`a_vault_relative_path_wikilink_resolves_to_the_note`,
`a_wikilink_in_a_headingless_note_is_indexed`,
`a_wikilink_resolves_against_the_target_notes_file_name`,
`an_anchor_relation_stays_scoped_to_the_note_it_names`.
