//! Runs the real `mct-index` pipeline against two Markdown fixtures:
//!
//! * `tests/fixtures/docs-project/` — a single small docs file with nested
//!   ATX headings, mirroring `crates/mct-lang-xml`'s integration test shape.
//! * `tests/fixtures/vault/` — a synthetic Obsidian vault exercising the
//!   conventions a real vault is made of: nested folders, file names with
//!   spaces, YAML front-matter, inline tags, and every wikilink spelling
//!   (`[[Note]]`, `[[Note.md]]`, `[[Note#Section]]`, `[[Note|alias]]`,
//!   `[[Note#Section|alias]]`, `![[Note]]`, `![[Note#Section]]`,
//!   `[[folder/Note]]`, and a dangling `[[Does Not Exist]]`).
//!
//! The vault tests answer one question: *does Obsidian actually work today?*
//! Several of them are `#[ignore]`d — each ignored test documents a real,
//! reproducible gap with its observed-versus-expected behavior, and is a
//! deliberate deliverable rather than a broken test.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-md/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, RelationHit};
use mct_lang_md::MarkdownParser;

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docs-project")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(MarkdownParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 1, "architecture.md");
    assert!(
        report.issues.is_empty(),
        "no parse issues expected: {:?}",
        report.issues
    );
    index
}

#[test]
fn find_symbol_locates_a_nested_heading() {
    let index = open_indexed();
    assert_eq!(index.find_symbol("Indexer").unwrap().len(), 1);
}

#[test]
fn find_symbol_reports_element_kind_and_correct_parent() {
    let index = open_indexed();
    let indexer = index.find_symbol("Indexer").unwrap();
    assert_eq!(indexer.len(), 1);
    assert_eq!(indexer[0].kind, "element");
    assert_eq!(indexer[0].parent.as_deref(), Some("Components"));
    // `language` is read straight off the `files.language` column via the
    // symbols->files JOIN — the aggregate `status()` check below can't tell
    // which individual row it came from, this can.
    assert_eq!(indexer[0].relative_path, "architecture.md");
    assert_eq!(indexer[0].language, "markdown");
}

#[test]
fn a_second_top_level_heading_does_not_inherit_the_first_ones_hierarchy() {
    let index = open_indexed();
    let unit_tests = index.find_symbol("Unit Tests").unwrap();
    assert_eq!(unit_tests.len(), 1);
    assert_eq!(unit_tests[0].parent.as_deref(), Some("Testing"));
}

#[test]
fn status_reports_full_markdown_coverage() {
    let index = open_indexed();
    let status = index.status().unwrap();
    let md = status
        .languages
        .iter()
        .find(|l| l.language == "markdown")
        .unwrap();
    assert_eq!(md.file_count, 1);
    assert_eq!(
        md.symbol_count, 7,
        "Architecture, Overview, Components, Indexer, Parser, Testing, Unit Tests"
    );
}

#[test]
fn a_wikilink_in_the_fixture_resolves_as_a_references_relation() {
    let index = open_indexed();
    let hits = index.find_references("Parser").unwrap();
    assert!(
        hits.iter()
            .any(|h| h.from_symbol == "Indexer" && h.kind == "references"),
        "{hits:?}"
    );
}

#[test]
fn a_tag_in_the_fixture_resolves_as_a_tag_prefixed_reference() {
    let index = open_indexed();
    let hits = index.find_references("tag:core").unwrap();
    assert!(hits.iter().any(|h| h.from_symbol == "Indexer"), "{hits:?}");
}

#[test]
fn a_wikilink_anchors_heading_part_is_discoverable_by_name() {
    let index = open_indexed();
    // `[[Parser#Testing]]` must index its note part exactly like a plain
    // `[[Parser]]` link (already covered by the test above) and must ALSO
    // index its heading part as an independent relation, discoverable by
    // `find_references("Testing")` — "Testing" is a real H1 heading in this
    // fixture. Note: the public `Index`/`RelationHit` API has no accessor
    // for the resolved `to_symbol_id` column, so this only proves the
    // relation is stored and discoverable by name (exactly what
    // `find_references` guarantees regardless of resolution), not that the
    // foreign key itself is non-NULL.
    let hits = index.find_references("Testing").unwrap();
    assert!(
        hits.iter()
            .any(|h| h.from_symbol == "Indexer" && h.kind == "references"),
        "{hits:?}"
    );
}

#[test]
fn a_wikilink_to_a_nonexistent_target_is_still_discoverable_by_name() {
    let index = open_indexed();
    // No heading named "Nonexistent Page" exists anywhere in the fixture, so
    // `to_symbol_id` stays NULL at insert time — but the relation row is
    // still inserted unconditionally, and `find_references` matches purely
    // on the `to_name` string column, never on `to_symbol_id`. Pre-existing
    // `mct-index` behavior, not new machinery; reconfirmed here because
    // Phase 3 introduces a new way (the heading-part split) to end up with
    // an unresolved name.
    let hits = index.find_references("Nonexistent Page").unwrap();
    assert!(
        hits.iter()
            .any(|h| h.from_symbol == "Indexer" && h.kind == "references"),
        "{hits:?}"
    );
}

// ---------------------------------------------------------------------------
// Obsidian vault round trip
// ---------------------------------------------------------------------------

fn vault_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vault")
}

/// Indexes `tests/fixtures/vault/` into an in-memory SQLite index. Nothing is
/// written to disk, so this can never touch the repository's own
/// `.mct-index/index.sqlite3`.
fn open_vault() -> Index {
    let root = vault_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(MarkdownParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(
        report.issues.is_empty(),
        "no parse issues expected across the vault: {:?}",
        report.issues
    );
    index
}

/// Every relation in the vault that targets `to_name` and originates from the
/// heading symbol `from_symbol`.
fn vault_refs_from(index: &Index, to_name: &str, from_symbol: &str) -> Vec<RelationHit> {
    index
        .find_references(to_name)
        .unwrap()
        .into_iter()
        .filter(|h| h.from_symbol == from_symbol)
        .collect()
}

#[test]
fn the_vault_indexes_every_note_without_parse_issues() {
    let index = open_vault();
    let status = index.status().unwrap();
    let md = status
        .languages
        .iter()
        .find(|l| l.language == "markdown")
        .unwrap();
    assert_eq!(
        md.file_count, 7,
        "index.md, orphan.md, daily/2026-09-18.md (front-matter), \
         notes/{{Glossary, Project Alpha, Untitled Capture}}.md and \
         notes/deep/Deep Note.md — nested folders and file names with spaces \
         must all be picked up"
    );
    // Front-matter, spaces in file names and deep nesting must not produce a
    // `ParseError::Syntax` — asserted by `open_vault` itself.
    assert!(md.symbol_count > 0);
}

#[test]
fn a_note_is_represented_by_its_top_level_heading() {
    let index = open_vault();
    // There is no "note"/"file" symbol kind — the whole wikilink graph hangs
    // off heading symbols, so a note's H1 is its stand-in.
    for (name, path) in [
        ("Vault Index", "index.md"),
        ("Glossary", "notes/Glossary.md"),
        ("Project Alpha", "notes/Project Alpha.md"),
        ("Deep Note", "notes/deep/Deep Note.md"),
        ("Daily 2026-09-18", "daily/2026-09-18.md"),
    ] {
        let hits = index.find_symbol(name).unwrap();
        assert_eq!(hits.len(), 1, "exactly one `{name}`: {hits:?}");
        assert_eq!(hits[0].kind, "element");
        assert_eq!(hits[0].parent, None, "`{name}` is a top-level heading");
        assert_eq!(hits[0].relative_path, path);
    }
}

#[test]
fn headings_at_every_level_become_nested_element_symbols() {
    let index = open_vault();
    // `notes/deep/Deep Note.md` is a straight H1..H6 ladder; `tree-sitter-md`'s
    // own `section` nesting must reproduce it exactly as a parent chain.
    for (child, parent) in [
        ("Level Two", "Deep Note"),
        ("Level Three", "Level Two"),
        ("Level Four", "Level Three"),
        ("Level Five", "Level Four"),
        ("Level Six", "Level Five"),
    ] {
        let hits = index.find_symbol(child).unwrap();
        assert_eq!(hits.len(), 1, "exactly one `{child}`: {hits:?}");
        assert_eq!(hits[0].kind, "element");
        assert_eq!(hits[0].parent.as_deref(), Some(parent));
    }
}

#[test]
fn a_plain_wikilink_becomes_a_references_relation_from_the_enclosing_heading() {
    let index = open_vault();
    // `## Reading Order` contains `[[Project Alpha]]` and `[[Glossary]]`.
    let alpha = vault_refs_from(&index, "Project Alpha", "Reading Order");
    assert_eq!(alpha.len(), 1, "{alpha:?}");
    assert_eq!(alpha[0].kind, "references");
    assert_eq!(alpha[0].relative_path, "index.md");
    assert_eq!(
        vault_refs_from(&index, "Glossary", "Reading Order").len(),
        1
    );
}

#[test]
fn a_wikilink_with_and_without_the_md_extension_resolve_identically() {
    let index = open_vault();
    // `## Extension Forms` holds `[[Glossary.md]]` and `[[Glossary]]` in one
    // paragraph; both must normalize to the bare target `Glossary`.
    let hits = vault_refs_from(&index, "Glossary", "Extension Forms");
    assert_eq!(hits.len(), 2, "both spellings recorded: {hits:?}");
    assert!(hits.iter().all(|h| h.to_name == "Glossary"));
    assert!(
        index.find_references("Glossary.md").unwrap().is_empty(),
        "the `.md` spelling must never survive into the index"
    );
}

#[test]
fn an_alias_resolves_to_the_target_and_never_to_the_display_text() {
    let index = open_vault();
    // `[[Project Alpha|the flagship effort]]` in `## Anchors And Aliases`.
    assert!(
        !vault_refs_from(&index, "Project Alpha", "Anchors And Aliases").is_empty(),
        "the alias link must point at the target note"
    );
    assert!(
        index
            .find_references("the flagship effort")
            .unwrap()
            .is_empty(),
        "the display text is presentation, never a relation target"
    );
}

#[test]
fn an_anchor_link_emits_a_relation_to_both_the_note_and_the_heading() {
    let index = open_vault();
    // `[[Project Alpha#Milestones]]` in `## Anchors And Aliases` splits into
    // two independent `References` relations (see the Phase 3 paragraph of
    // `mct_lang_md`'s module doc for why it is two and not one composite).
    assert!(!vault_refs_from(&index, "Project Alpha", "Anchors And Aliases").is_empty());
    let heading = vault_refs_from(&index, "Milestones", "Anchors And Aliases");
    assert_eq!(heading.len(), 1, "{heading:?}");
    assert_eq!(heading[0].to_name, "Milestones");
    // And the heading part really does name a symbol that exists.
    assert_eq!(index.find_symbol("Milestones").unwrap().len(), 1);
}

#[test]
fn an_anchor_with_an_alias_resolves_both_parts_and_drops_the_alias() {
    let index = open_vault();
    // `[[Glossary#Terminology|the vocabulary section]]`.
    assert!(!vault_refs_from(&index, "Glossary", "Anchors And Aliases").is_empty());
    assert!(!vault_refs_from(&index, "Terminology", "Anchors And Aliases").is_empty());
    assert!(index
        .find_references("the vocabulary section")
        .unwrap()
        .is_empty());
}

#[test]
fn an_anchor_link_reaches_a_heading_in_another_note() {
    let index = open_vault();
    // `daily/2026-09-18.md` links `[[Glossary#Symbol]]`; `Symbol` is an H3
    // inside `notes/Glossary.md`, two files away.
    let hits = vault_refs_from(&index, "Symbol", "Daily 2026-09-18");
    assert_eq!(hits.len(), 1, "{hits:?}");
    let target = index.find_symbol("Symbol").unwrap();
    assert_eq!(target.len(), 1);
    assert_eq!(target[0].relative_path, "notes/Glossary.md");
    assert_eq!(target[0].parent.as_deref(), Some("Terminology"));
}

#[test]
fn an_embed_is_recorded_as_a_relation() {
    let index = open_vault();
    // `## Embeds` holds `![[Glossary]]` and `![[Glossary#Terminology]]` in two
    // separate paragraphs — three relations in total.
    let note = vault_refs_from(&index, "Glossary", "Embeds");
    assert_eq!(note.len(), 2, "one per embed: {note:?}");
    let section = vault_refs_from(&index, "Terminology", "Embeds");
    assert_eq!(section.len(), 1, "{section:?}");
}

#[test]
fn a_wikilink_inside_a_list_item_is_indexed() {
    let index = open_vault();
    // Bullet lists of links are the single most common shape in a real vault.
    assert_eq!(vault_refs_from(&index, "Glossary", "Link List").len(), 1);
    assert_eq!(vault_refs_from(&index, "Deep Note", "Link List").len(), 1);
}

#[test]
fn an_inline_tag_becomes_a_tag_prefixed_relation() {
    let index = open_vault();
    for (tag, from) in [
        ("tag:vault", "Vault Index"),
        ("tag:project/alpha", "Project Alpha"),
        ("tag:standup", "Daily 2026-09-18"),
        ("tag:review/weekly", "Daily 2026-09-18"),
    ] {
        let hits = vault_refs_from(&index, tag, from);
        assert_eq!(hits.len(), 1, "`{tag}` from `{from}`: {hits:?}");
        assert_eq!(hits[0].kind, "references");
    }
}

#[test]
fn an_unresolved_wikilink_is_recorded_by_name_and_does_not_break_the_index() {
    let index = open_vault();
    // `[[Does Not Exist]]` names no heading anywhere in the vault. The
    // relation row is still inserted with `to_name = "Does Not Exist"` and a
    // NULL `to_symbol_id`; indexing does not panic, does not report an issue
    // (asserted in `open_vault`), and the dangling edge stays queryable by
    // name — which is exactly how you would find broken links in a vault.
    let hits = index.find_references("Does Not Exist").unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].from_symbol, "Dangling");
    assert_eq!(hits[0].relative_path, "index.md");
    assert!(
        index.find_symbol("Does Not Exist").unwrap().is_empty(),
        "nothing defines that name — the edge is deliberately dangling"
    );
}

// ---------------------------------------------------------------------------
// Known gaps. Each test below fails today; the comment records observed vs
// expected. They are kept (ignored) as executable documentation of what an
// Obsidian vault still cannot do through this index.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "heading level is not stored: every heading is kind=element"]
fn a_heading_records_the_level_it_was_written_at() {
    let index = open_vault();
    // Observed: `Deep Note` (H1), `Level Two` (H2) ... `Level Six` (H6) all
    // come back with `kind == "element"`, and neither `SymbolHit` nor
    // `SymbolListEntry` carries a level/depth field, so `#` and `######` are
    // indistinguishable once indexed. Parent nesting is the only surviving
    // hierarchy signal, and it cannot be inverted into a level because a
    // document may skip levels (`##` directly followed by `####`).
    // Expected: an H1 and an H6 should be distinguishable in the index.
    let h1 = index.find_symbol("Deep Note").unwrap();
    let h6 = index.find_symbol("Level Six").unwrap();
    assert_ne!(
        h1[0].kind, h6[0].kind,
        "an H1 and an H6 must not be indexed as the same thing"
    );
}

#[test]
#[ignore = "an embed is indexed identically to a plain link"]
fn an_embed_is_distinguishable_from_a_plain_wikilink() {
    let index = open_vault();
    // Observed: `![[Glossary]]` and `[[Glossary]]` both produce
    // `RelationKind::References` with `to_name = "Glossary"`. The leading `!`
    // is outside the span `scan_wikilinks` matches and is never recorded, so
    // "transclude this note here" and "mention this note" collapse into one
    // edge kind. Expected: a distinct relation kind (or a flag) for embeds,
    // since an embed changes a note's rendered content and a link does not.
    let embed = vault_refs_from(&index, "Glossary", "Embeds");
    let plain = vault_refs_from(&index, "Glossary", "Reading Order");
    assert_ne!(embed[0].kind, plain[0].kind, "embed vs link must differ");
}

#[test]
#[ignore = "YAML front-matter is skipped entirely, tags included"]
fn front_matter_tags_are_indexed() {
    let index = open_vault();
    // Observed: `daily/2026-09-18.md` opens with a `---` block declaring
    // `tags: [daily, review]`. The file parses cleanly (no issue is
    // reported), but `find_references("tag:daily")` and
    // `find_references("tag:review")` both return zero rows — the parser only
    // scans `atx_heading` text and `paragraph` nodes, and front-matter is
    // neither. Inline `#standup` in the body IS indexed, so a vault that
    // tags in front-matter (the Obsidian default for many templates) gets no
    // tag graph at all. Expected: front-matter tags indexed like inline ones.
    assert!(!index.find_references("tag:daily").unwrap().is_empty());
    assert!(!index.find_references("tag:review").unwrap().is_empty());
}

#[test]
#[ignore = "a vault-relative path wikilink is not normalized to the note name"]
fn a_vault_relative_path_wikilink_resolves_to_the_note() {
    let index = open_vault();
    // Observed: `[[notes/Glossary]]` in `## Path Forms` is stored verbatim as
    // `to_name = "notes/Glossary"`, which matches no symbol — only
    // `strip_md_extension` runs on the note part, never a path-tail split.
    // Obsidian treats `[[notes/Glossary]]` and `[[Glossary]]` as the same
    // note, so this silently drops a real edge.
    // Expected: the same target as a bare `[[Glossary]]`.
    let path_form = index.find_references("notes/Glossary").unwrap();
    assert!(path_form.is_empty(), "the raw path form must not survive");
    assert!(
        !vault_refs_from(&index, "Glossary", "Path Forms").is_empty(),
        "a path-qualified link must resolve to the note it names"
    );
}

#[test]
#[ignore = "a link outside any heading is silently dropped"]
fn a_wikilink_in_a_headingless_note_is_indexed() {
    let index = open_vault();
    // Observed: `orphan.md` has no heading at all, so it produces zero
    // symbols; `push_relations_from_text` needs an enclosing heading symbol
    // to hang a relation off, and a paragraph with `parent_id == None` is
    // skipped. Its `[[Glossary]]` link and `#orphan` tag vanish without a
    // trace or a reported issue. Heading-less capture notes are routine in a
    // real vault, so this is a whole class of lost edges.
    // Expected: a file-level symbol (or equivalent) to anchor such links.
    assert!(
        !index.find_references("tag:orphan").unwrap().is_empty(),
        "the tag in orphan.md must be reachable"
    );
    assert!(
        index
            .find_references("Glossary")
            .unwrap()
            .iter()
            .any(|h| h.relative_path == "orphan.md"),
        "the link in orphan.md must be reachable"
    );
}

#[test]
#[ignore = "a note is only addressable by its H1, never by its file name"]
fn a_wikilink_resolves_against_the_target_notes_file_name() {
    let index = open_vault();
    // Observed: `notes/Untitled Capture.md` starts with `# A Different Title`,
    // so the only symbol it contributes is named "A Different Title". The
    // link `[[Untitled Capture]]` in `## Filename Mismatch` is recorded but
    // matches nothing — this crate has no file/note symbol, and whole-note
    // resolution relies entirely on the convention that a note's H1 equals
    // its file name. Every renamed or untitled note in a real vault breaks it.
    // Expected: `[[Untitled Capture]]` reaches `notes/Untitled Capture.md`.
    assert!(
        !vault_refs_from(&index, "Untitled Capture", "Filename Mismatch").is_empty(),
        "the link itself is recorded"
    );
    assert_eq!(
        index.find_symbol("Untitled Capture").unwrap().len(),
        1,
        "the note must be addressable by its file name"
    );
}

#[test]
#[ignore = "an anchor's heading part is a bare name, unscoped to its note"]
fn an_anchor_relation_stays_scoped_to_the_note_it_names() {
    let index = open_vault();
    // Observed: `[[Project Alpha#Goals]]` (in `notes/deep/Deep Note.md`) is
    // split into two independent relations, `-> "Project Alpha"` and
    // `-> "Goals"`. Nothing ties the second to the first, and this vault
    // deliberately contains two `## Goals` headings (in `notes/Project
    // Alpha.md` and `notes/Glossary.md`), so the heading edge is ambiguous by
    // construction. Querying the composite name returns nothing at all.
    // Expected: the anchor resolves to exactly one heading, the one in the
    // note the link named.
    assert_eq!(
        index.find_symbol("Goals").unwrap().len(),
        2,
        "precondition: the vault has two same-named headings"
    );
    assert!(
        !index
            .find_references("Project Alpha#Goals")
            .unwrap()
            .is_empty(),
        "the note-qualified anchor must be queryable as one target"
    );
}
