# Pending issues

Product bugs found by the test-coverage audit (2026-09-18) and deliberately
left **unfixed**. Each one is pinned by an `#[ignore]`d test that documents the
observed-versus-expected behavior; the ignored tests are the specification for
the fix, so a fix is done when its test passes with the `#[ignore]` removed.

Run them with `cargo test --workspace -- --ignored`.

Every `#[ignore]`d test that pins a bug must be listed here, with its name in
backticks; `crates/mct-mcp-server/tests/repo_ledgers.rs` fails CI otherwise. A
test parked on purpose (a report, a benchmark) gives the reason as
`#[ignore = "not a bug: …"]` and is not listed here.

---

## 1. ~~`get_file_skeleton` / `get_project_overview` are blind to 4 of 17 languages~~ — **closed**

Fixed on `fix/skeleton-parented-languages`: both tools now share one rule,
`is_top_level` in `crates/mct-mcp-server/src/server.rs`. A parent counts as a
file-level container when it is a `module` symbol that no other symbol in the
file shares by name. The two previously ignored tests now pass without
`#[ignore]`, and the two tests that pinned the broken output were replaced.

The description below is the original report, kept as a historical snapshot.

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

## 2. ~~`.lua` files are dropped silently, with no diagnostic at all~~ — **closed**

Lua shipped: both production registries (`mct-mcp-server/src/registry.rs` and
`mct-cli/src/main.rs`) register `mct_lang_lua::LuaParser` (commit `11df400`, merged
into `main` through PR #91). The test that pinned the hole was replaced.

**Covered by**:
- `a_lua_file_is_indexed_with_its_symbols_and_calls` (`crates/mct-mcp-server/tests/omni_fixture.rs`)
- `lua_definitions_and_calls_are_served_through_the_mcp_tools` (`crates/mct-mcp-server/tests/omni_tools.rs`)

---

## 3. Obsidian-vault Markdown: seven gaps in the note/link graph — **closed**

3.1 closed with issue #10. 3.2–3.7 closed by the note graph of issue #98
(design: `docs/superpowers/specs/2026-10-01-markdown-note-graph-design.md`),
merged into `main` through PR #101 on top of the relation identity of
issue #97 / PR #99. Every note is a `module` symbol named by its file stem, and
wikilinks resolve by path and note scope. The original observations below
describe the code before that change.

Pinned in `crates/mct-lang-md/tests/index_integration.rs` (vault:
`crates/mct-lang-md/tests/fixtures/vault/`) unless marked `note_graph.rs`
(`crates/mct-lang-md/tests/note_graph.rs`). None of these tests is ignored.

| # | Gap (as originally observed) | Resolution | Covered by |
|---|-----|----------|----------|
| 3.1 | Heading level is not stored | `level: Option<u32>` (1..6) on symbols; `kind` stays `element` (issue #10) | `a_heading_records_the_level_it_was_written_at` |
| 3.2 | Embeds collapse into links: `![[Glossary]]` and `[[Glossary]]` were both `References` | Embeds are `Imports`, links stay `References` | `an_embed_is_distinguishable_from_a_plain_wikilink` |
| 3.3 | YAML front-matter skipped: `tags: [daily, review]` produced no `tag:` reference | Front-matter `tags`, `aliases` and `title` become `tag:`/`alias:`/`title:` references from the note | `front_matter_tags_aliases_and_title_are_queryable` (asserts `tag:` and `title:`; `alias:` is not asserted) |
| 3.4 | `[[notes/Glossary]]` stored verbatim, matching nothing | A path-written link carries the exact note path and resolves to exactly that note | `a_vault_relative_path_wikilink_resolves_to_exactly_that_note`; `note_graph.rs`: `duplicate_basenames_are_ambiguous_but_a_path_picks_one` |
| 3.5 | Links/tags in heading-less notes dropped | The note symbol owns them; heading-less and empty files are notes | `a_link_in_a_headingless_note_belongs_to_the_note`; `note_graph.rs`: `a_heading_less_or_empty_file_is_a_searchable_note` |
| 3.6 | A note was addressable only by its H1 | Notes resolve by file stem; a heading title is not a note identity | `a_wikilink_resolves_against_the_target_notes_file_name`, `a_note_is_not_addressable_by_its_heading_title` |
| 3.7 | `[[Project Alpha#Goals]]` lost its note scope | The heading resolves only inside the named note; a same-named heading elsewhere is never a fallback | `an_anchor_relation_stays_scoped_to_the_note_it_names`; `note_graph.rs`: `two_notes_with_the_same_shared_heading_do_not_cross_link`, `a_heading_in_another_note_is_never_a_fallback` |

### Remaining Markdown limitations (documented, not bugs)

Recorded in the "Implementation notes" of the note-graph design; none has an
ignored test:
- A wikilink is not resolved by alias or title: `[[Alias]]` stays `unresolved`.
- `^block` anchors are ignored; heading anchors match the exact heading text
  (no slug or case folding, last `#` segment only).
- Link paths are case-sensitive and relative to the vault root or the source
  note (`./`, `../`); Obsidian's shortest-suffix paths are not resolved.
- Only `title`, `aliases` and `tags` front-matter keys are read.
- HTML comments and `%%comments%%` are not skipped (inline code and fenced
  blocks are).

---

## 4. Incremental reindex keeps a syntax error after a file turns binary

**Observed**: after `src/util.fake` is replaced by binary content, the index
still reports `src/util.fake line 1: expected `fn`, got `not fn`` as an error.
A full reindex keeps it too.

**Expected**: no errors, because the file no longer parses as source.

**Pinned by** `a_broken_file_turned_binary_drops_its_syntax_error`
(`crates/mct-index/tests/incremental.rs`).

---

## 5. Bash: a named `coproc` truncates the enclosing function (#79)

**Observed**: the function that contains `coproc NAME { … }` ends at line 227
instead of 236, because tree-sitter-bash 0.25 misparses the construct.

**Expected**: the function spans its full body.

**Tracked in** #79. **Pinned by** `named_coproc_keeps_the_enclosing_function_intact`
(`crates/mct-lang-bash/tests/corpus.rs`).

---

## 6. Java: `this(…)`, `super(…)` and method references leave no relation

**Observed**: no `calls` relation from `summing` to `plus` for the method
reference `Money::plus`. The `this(…)`/`super(…)` chaining has the same gap.

**Expected**: both produce relations, as ordinary calls do.

**Issue**: not filed yet. **Pinned by**
`constructor_chaining_and_method_references_are_relations`
(`crates/mct-lang-java/tests/corpus.rs`).

---

## 7. Kotlin: companion objects and `typealias` leave no symbol

**Observed**: the companion object of a class (`Factory`) and the `typealias`
declarations are not indexed as symbols.

**Expected**: each gets a symbol, as the other declarations do.

**Issue**: not filed yet. **Pinned by**
`companion_objects_and_type_aliases_are_symbols`
(`crates/mct-lang-kotlin/tests/corpus.rs`).

---

## 8. Rust: a method call inside a macro's arguments leaves no relation

**Observed**: in `assert_eq!(cli.for_extension(e), server.for_extension(e))`
neither call leaves a relation, because tree-sitter hands the macro's
arguments over as a flat `token_tree` and the extractor skips identifiers
after `.`/`::` there. `find_callers`, `find_references` and `impact_analysis`
report 3 of the 5 real call sites of `LanguageRegistry::for_extension` (counted
before that CLI/MCP-server parity test was removed with the shared
`mct-languages` registry; the test below keeps its shape).

**Expected**: a relation for each call, at least a `references` one. See
`benchmarks/agent-benchmark.md#calls-inside-macros` for the recommended fix.

**Issue**: not filed yet. **Pinned by**
`a_method_call_inside_a_macro_token_tree_leaves_a_relation`
(`crates/mct-lang-rust/tests/parse.rs`).

---

## 9. Rust: a path in value position leaves no relation

**Observed**: `registry.register(Arc::new(mct_lang_rust::RustParser))` leaves
only `calls` to `register` and `new`. The walker's `scoped_identifier` arm
records nothing, so `find_references RustParser` found none of the registry
uses when the registry moved to `mct-languages`; the compiler did. A call
through the same path (`m::f()`) is already recorded with `module` evidence.
Tracked as `errors/REGISTRY.md` ERR-010.

**Expected**: a `references` relation with `module` evidence
(`mct_lang_rust`), so it never resolves to a same-named item of this file.
The fix changes the `by_path` case of
`locals_fields_paths_labels_and_foreign_names_are_not_references` and the
Rust corpus snapshot, to be reviewed together.

**Issue**: not filed yet. **Pinned by**
`a_path_in_value_position_is_a_reference_qualified_by_its_module`
(`crates/mct-lang-rust/tests/parse.rs`).
