//! Long, multi-file fixture corpus (issue #74): the engineering notes of a
//! warehouse team as an Obsidian-style vault — a handbook, an architecture
//! overview and decision log, an operations runbook, an API reference and a
//! glossary — each file 300–600 lines and linking the others by wiki link,
//! section anchor, alias, relative path and embed. The shared checks (size,
//! line ranges, golden snapshot, index round trip, malformed input) come from
//! `mct-corpus`; the tests below pin the note graph Markdown is expected to
//! produce and its documented limits.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-md/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_md::MarkdownParser;

mct_corpus::standard_tests!(MarkdownParser);

use RelationKind::{Imports, References};

fn heading<'a>(path: &str, name: &str) -> &'a mct_core::SymbolRecord {
    corpus().symbol(path, name, SymbolKind::Element)
}

fn parent<'a>(path: &str, name: &str) -> Option<&'a str> {
    heading(path, name).parent.as_deref()
}

fn lines(path: &str, name: &str) -> (u32, Option<u32>) {
    let h = heading(path, name);
    (h.location.line, h.location.end_line)
}

#[test]
fn every_note_is_a_module_named_after_its_file() {
    for (path, name) in [
        ("handbook.md", "handbook"),
        ("glossary.md", "glossary"),
        ("architecture/overview.md", "overview"),
        ("architecture/decisions.md", "decisions"),
        ("operations/runbook.md", "runbook"),
        ("api/reference.md", "reference"),
    ] {
        let note = corpus().symbol(path, name, SymbolKind::Module);
        assert_eq!(
            (note.location.line, note.parent.as_deref()),
            (1, None),
            "{path}"
        );
    }
}

#[test]
fn headings_nest_by_level() {
    assert_eq!(
        parent("handbook.md", "Warehouse engineering handbook"),
        None
    );
    assert_eq!(
        parent("handbook.md", "Who we are"),
        Some("Warehouse engineering handbook")
    );
    assert_eq!(parent("handbook.md", "Team members"), Some("Who we are"));
    assert_eq!(
        parent("handbook.md", "Configuration"),
        Some("Development environment")
    );
    assert_eq!(
        parent("runbook.md", "Orders stuck in RESERVED"),
        Some("Common symptoms")
    );
    assert_eq!(
        parent("glossary.md", "Reservation expiry"),
        Some("Reservation")
    );
    // Headings record the level they were written at.
    assert_eq!(heading("runbook.md", "Common symptoms").level, Some(3));
    assert_eq!(
        heading("runbook.md", "Orders stuck in RESERVED").level,
        Some(4)
    );
}

#[test]
fn setext_and_atx_headings_share_one_hierarchy() {
    // `====` and `----` underlines are levels 1 and 2…
    assert_eq!(
        heading("decisions.md", "Architecture decision log").level,
        Some(1)
    );
    assert_eq!(
        parent("decisions.md", "Decision log"),
        Some("Architecture decision log")
    );
    // …and an ATX `###` under a setext `----` nests inside it.
    assert_eq!(
        parent("decisions.md", "ADR-001 Modular monolith"),
        Some("Decision log")
    );
    // Every ADR has its own `#### Context`, one symbol per ADR.
    let owners: Vec<_> = corpus()
        .symbols_named("Context")
        .into_iter()
        .filter(|(p, _)| p.ends_with("decisions.md"))
        .map(|(_, s)| s.parent.as_deref().unwrap())
        .collect();
    assert_eq!(owners.len(), 10);
    assert!(owners.iter().all(|o| o.starts_with("ADR-")), "{owners:?}");
    // A setext heading after ATX sections closes them.
    assert_eq!(
        parent("handbook.md", "Glossary of acronyms"),
        Some("Warehouse engineering handbook")
    );
}

#[test]
fn a_section_runs_to_the_next_heading_of_the_same_or_higher_level() {
    let (start, end) = lines("handbook.md", "Who we are");
    let (next, _) = lines("handbook.md", "Getting started");
    assert_eq!(end, Some(next - 1));
    assert!(next - start > 30, "the section contains its subsections");
    // The last section runs to the end of the note.
    let (_, end) = lines("glossary.md", "Spanish ↔ English");
    assert_eq!(end, Some(316));
}

#[test]
fn wiki_links_reference_notes_and_their_sections() {
    let c = corpus();
    let from = "Warehouse engineering handbook";
    // `[[architecture/overview|architecture overview]]`: the alias is dropped.
    c.relation("handbook.md", from, References, "overview");
    c.relation("handbook.md", from, References, "glossary");
    // `[[operations/runbook#Post-incident review]]`: note and section.
    c.relation("handbook.md", "Rituals", References, "runbook");
    c.relation("handbook.md", "Rituals", References, "Post-incident review");
    // `[[#Planning]]`: a section of the same note.
    c.relation("handbook.md", "Rituals", References, "Planning");
    // Relative paths resolve against the note's folder.
    c.relation(
        "overview.md",
        "Architecture overview",
        References,
        "decisions",
    );
    c.relation(
        "overview.md",
        "Architecture overview",
        References,
        "runbook",
    );
    // A link to a note that does not exist yet is still a reference.
    c.relation(
        "handbook.md",
        "Notes about these notes",
        References,
        "roadmap-2027",
    );
    // `[[#ADR-008 Canary releases|ADR-008]]` keeps the section, not the alias.
    c.relation(
        "decisions.md",
        "Superseded decisions",
        References,
        "ADR-008 Canary releases",
    );
    assert!(!c.has_relation("Superseded decisions", References, "ADR-008"));
}

#[test]
fn embeds_are_imports() {
    let c = corpus();
    c.relation("handbook.md", "Embedding", Imports, "runbook");
    c.relation("handbook.md", "Embedding", Imports, "On-call rota");
    c.relation("handbook.md", "Embedding", Imports, "context-diagram.svg");
    c.relation("overview.md", "Context", Imports, "context-diagram.svg");
}

#[test]
fn frontmatter_and_inline_tags_are_tag_references() {
    let c = corpus();
    for tag in ["tag:home", "tag:team/warehouse", "tag:onboarding"] {
        c.relation("handbook.md", "handbook", References, tag);
    }
    c.relation(
        "handbook.md",
        "handbook",
        References,
        "alias:Manual de ingeniería",
    );
    c.relation(
        "handbook.md",
        "handbook",
        References,
        "title:Warehouse engineering handbook",
    );
    c.relation("glossary.md", "glossary", References, "alias:Glosario");
    // `#domain` quoted in YAML; the bare `2026` is a number, not a tag.
    c.relation("glossary.md", "glossary", References, "tag:domain");
    assert!(!c.has_relation("glossary", References, "tag:2026"));
    // Inline tags, Unicode included, attach to the enclosing section.
    c.relation(
        "handbook.md",
        "Getting started",
        References,
        "tag:onboarding",
    );
    c.relation(
        "glossary.md",
        "Spanish ↔ English",
        References,
        "tag:almacén",
    );
}

#[test]
fn code_tables_and_look_alikes_are_not_links_or_tags() {
    let c = corpus();
    let targets: Vec<_> = c.relations().into_iter().map(|r| r.to).collect();
    for absent in [
        // Inline code spans and fenced blocks.
        "not-a-link",
        "this-is-not-a-link",
        "tag:not-a-tag",
        "tag:this-is-not-a-tag",
        "note",
        // All-digit tags and URL fragments.
        "tag:1234",
        "tag:install",
    ] {
        assert!(!targets.contains(&absent), "{absent} was extracted");
    }
    // Table cells are not scanned: the team table links the overview, but
    // its section has no relation.
    assert!(!c.has_relation("Team members", References, "overview"));
}

#[test]
fn notes_link_each_other_across_files() {
    let c = corpus();
    assert!(c.cross_file_relation_count() >= 100);
    // A section anchor in another note is resolved by name.
    c.relation(
        "runbook.md",
        "Orders stuck in RESERVED",
        References,
        "ADR-003 Reserve before payment",
    );
    c.relation(
        "reference.md",
        "Carrier webhooks",
        References,
        "ADR-006 One adapter per carrier",
    );
}

#[test]
fn index_answers_note_graph_queries() {
    let index = corpus().index();
    let refs = index.find_references("runbook").unwrap();
    let mut files: Vec<_> = refs.iter().map(|r| r.relative_path.as_str()).collect();
    files.sort();
    files.dedup();
    for file in [
        "api/reference.md",
        "architecture/decisions.md",
        "architecture/overview.md",
        "glossary.md",
        "handbook.md",
    ] {
        assert!(files.contains(&file), "{file} links the runbook: {files:?}");
    }
    let refs = index.find_references("tag:money").unwrap();
    assert!(refs.len() >= 5);
}
