// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-md/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_md::MarkdownParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    MarkdownParser
        .parse(&SourceFile {
            relative_path: "doc.md".to_string(),
            contents: src.to_string(),
        })
        .expect("valid Markdown source should parse")
}

#[test]
fn h1_heading_becomes_an_element_symbol() {
    let parsed = parse("# A\n");
    let a = parsed.symbols.iter().find(|s| s.name == "A").unwrap();
    assert_eq!(a.kind, SymbolKind::Element);
}

#[test]
fn symbol_name_is_the_heading_text() {
    let parsed = parse("# Getting Started\n");
    assert!(parsed.symbols.iter().any(|s| s.name == "Getting Started"));
}

#[test]
fn h2_is_associated_with_its_h1() {
    let parsed = parse("# A\n\n## B\n");
    let b = parsed.symbols.iter().find(|s| s.name == "B").unwrap();
    assert_eq!(b.parent.as_deref(), Some("A"));
}

#[test]
fn two_consecutive_h2s_are_siblings() {
    let parsed = parse("# A\n\n## B\n\n## C\n");
    let b = parsed.symbols.iter().find(|s| s.name == "B").unwrap();
    let c = parsed.symbols.iter().find(|s| s.name == "C").unwrap();
    assert_eq!(b.parent.as_deref(), Some("A"));
    assert_eq!(c.parent.as_deref(), Some("A"));
    // Siblings, not nested under each other.
    assert_ne!(c.parent.as_deref(), Some("B"));
}

#[test]
fn h3_is_associated_with_its_h2() {
    let parsed = parse("# A\n\n## B\n\n### C\n");
    let c = parsed.symbols.iter().find(|s| s.name == "C").unwrap();
    assert_eq!(c.parent.as_deref(), Some("B"));
}

#[test]
fn a_new_h1_starts_a_new_hierarchy() {
    let parsed = parse("# A\n\n## B\n\n# E\n\n## F\n");
    let f = parsed.symbols.iter().find(|s| s.name == "F").unwrap();
    // F is under the new H1 (E), never under A or A's subtree.
    assert_eq!(f.parent.as_deref(), Some("E"));
    assert_ne!(f.parent.as_deref(), Some("A"));
    assert_ne!(f.parent.as_deref(), Some("B"));
}

#[test]
fn all_six_heading_levels_are_recognized() {
    let parsed = parse("# H1\n\n## H2\n\n### H3\n\n#### H4\n\n##### H5\n\n###### H6\n");
    for name in ["H1", "H2", "H3", "H4", "H5", "H6"] {
        assert!(
            parsed
                .symbols
                .iter()
                .any(|s| s.name == name && s.kind == SymbolKind::Element),
            "expected an Element symbol named {name}, got {:?}",
            parsed.symbols
        );
    }
    assert_eq!(parsed.symbols.len(), 6);
}

#[test]
fn document_without_headings_produces_no_symbols() {
    let parsed = parse("Just a paragraph of text.\n\nAnother paragraph, still no headings.\n");
    assert!(parsed.symbols.is_empty(), "{:?}", parsed.symbols);
}

/// The exact case from the Phase 1 spec: `# A / ## B / ## C / ### D` (under
/// C, not B) `/ # E / ## F` (a fresh hierarchy, not nested under A at all).
#[test]
fn full_abcdef_hierarchy_matches_the_spec_example() {
    let parsed = parse("# A\n\n## B\n\n## C\n\n### D\n\n# E\n\n## F\n");

    let by_name = |name: &str| parsed.symbols.iter().find(|s| s.name == name).unwrap();

    assert_eq!(by_name("A").parent, None);
    assert_eq!(by_name("B").parent.as_deref(), Some("A"));
    assert_eq!(by_name("C").parent.as_deref(), Some("A"));
    assert_eq!(
        by_name("D").parent.as_deref(),
        Some("C"),
        "D must nest under C, not B"
    );
    assert_eq!(
        by_name("E").parent,
        None,
        "E starts a fresh hierarchy, not nested under A"
    );
    assert_eq!(by_name("F").parent.as_deref(), Some("E"));

    assert!(
        parsed.relations.is_empty(),
        "no [[WikiLinks]] or #tags appear in this input: {:?}",
        parsed.relations
    );
}

#[test]
fn a_standard_markdown_link_is_not_mistaken_for_a_wikilink() {
    let parsed = parse("# A\n\nSome text with a [link](other.md) in it.\n\n## B\n");
    assert!(
        parsed.relations.is_empty(),
        "single-bracket links are not [[WikiLinks]]: {:?}",
        parsed.relations
    );
}

#[test]
fn wikilink_in_a_paragraph_emits_a_references_relation() {
    let parsed = parse("# A\n\nSee [[Other Note]] for details.\n");
    let a = parsed.symbols.iter().find(|s| s.name == "A").unwrap();
    let rel = parsed
        .relations
        .iter()
        .find(|r| r.to_name == "Other Note")
        .unwrap();
    assert_eq!(rel.kind, RelationKind::References);
    assert_eq!(rel.from, a.id);
}

#[test]
fn wikilink_alias_is_stripped_from_the_target() {
    let parsed = parse("# A\n\nSee [[Other Note|click here]] for details.\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Other Note"));
    assert!(!parsed
        .relations
        .iter()
        .any(|r| r.to_name.contains("click here")));
}

#[test]
fn wikilink_anchor_is_split_into_two_relations() {
    let parsed = parse("# A\n\nSee [[Other Note#Some Heading]] for details.\n");
    assert!(
        parsed.relations.iter().any(|r| r.to_name == "Other Note"),
        "{:?}",
        parsed.relations
    );
    assert!(
        parsed.relations.iter().any(|r| r.to_name == "Some Heading"),
        "{:?}",
        parsed.relations
    );
    assert!(
        !parsed
            .relations
            .iter()
            .any(|r| r.to_name == "Other Note#Some Heading"),
        "the anchor must no longer be indexed verbatim as one target: {:?}",
        parsed.relations
    );
}

#[test]
fn wikilink_anchor_and_alias_together_strip_alias_and_split_anchor() {
    let parsed = parse("# A\n\nSee [[Other Note#Some Heading|click here]] for details.\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Other Note"));
    assert!(parsed.relations.iter().any(|r| r.to_name == "Some Heading"));
    assert!(!parsed
        .relations
        .iter()
        .any(|r| r.to_name.contains("click here")));
}

#[test]
fn a_same_document_anchor_link_emits_only_the_heading_relation() {
    let parsed = parse("# A\n\nSee [[#Some Heading]] for details.\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Some Heading"));
    assert_eq!(
        parsed.relations.len(),
        1,
        "an empty note part must not produce a spurious relation: {:?}",
        parsed.relations
    );
}

#[test]
fn wikilink_md_suffix_is_normalized_away() {
    let parsed = parse("# A\n\nSee [[Other Note.md]] for details.\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Other Note"));
    assert!(
        !parsed.relations.iter().any(|r| r.to_name.contains(".md")),
        "{:?}",
        parsed.relations
    );
}

#[test]
fn wikilink_md_suffix_is_normalized_case_insensitively() {
    let parsed = parse("# A\n\nSee [[Other Note.MD]] for details.\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Other Note"));
}

#[test]
fn an_embed_is_scanned_identically_to_a_plain_wikilink() {
    let parsed = parse("# A\n\nSee ![[Other Note]] for details.\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Other Note"));
}

#[test]
fn an_embed_with_an_anchor_is_split_like_a_plain_wikilink() {
    let parsed = parse("# A\n\nSee ![[Other Note#Some Heading]] for details.\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Other Note"));
    assert!(parsed.relations.iter().any(|r| r.to_name == "Some Heading"));
}

#[test]
fn multiple_distinct_wikilinks_in_one_note_all_emit_relations() {
    let parsed = parse("# A\n\nSee [[Note One]] and [[Note Two]] for details.\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Note One"));
    assert!(parsed.relations.iter().any(|r| r.to_name == "Note Two"));
}

#[test]
fn a_repeated_wikilink_emits_a_relation_each_time_not_deduped() {
    let parsed = parse("# A\n\nSee [[Other Note]] and again [[Other Note]].\n");
    let count = parsed
        .relations
        .iter()
        .filter(|r| r.to_name == "Other Note")
        .count();
    assert_eq!(count, 2, "{:?}", parsed.relations);
}

#[test]
fn a_wikilink_without_an_extension_or_anchor_is_unaffected() {
    let parsed = parse("# A\n\nSee [[Other Note]] for details.\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Other Note"));
    assert_eq!(parsed.relations.len(), 1, "{:?}", parsed.relations);
}

#[test]
fn a_malformed_nested_wikilink_still_finds_the_well_formed_inner_link() {
    let parsed = parse("# A\n\na [[ b [[Real]] c\n");
    assert!(
        parsed.relations.iter().any(|r| r.to_name == "Real"),
        "{:?}",
        parsed.relations
    );
    assert!(
        !parsed.relations.iter().any(|r| r.to_name.contains("[[")),
        "no garbage target containing a stray `[[` should ever be emitted: {:?}",
        parsed.relations
    );
}

#[test]
fn unbalanced_single_brackets_are_not_mistaken_for_a_wikilink() {
    let parsed = parse("# A\n\nThis has a stray [bracket and ] but no wikilink.\n");
    assert!(parsed.relations.is_empty(), "{:?}", parsed.relations);
}

#[test]
fn wikilink_inside_the_heading_text_itself_emits_a_relation() {
    let parsed = parse("# See [[Other Note]]\n");
    let heading = parsed
        .symbols
        .iter()
        .find(|s| s.name == "See [[Other Note]]")
        .unwrap();
    let rel = parsed
        .relations
        .iter()
        .find(|r| r.to_name == "Other Note")
        .unwrap();
    assert_eq!(rel.from, heading.id);
}

#[test]
fn hashtag_in_a_paragraph_emits_a_tag_prefixed_relation() {
    let parsed = parse("# A\n\nThis note is about #productivity today.\n");
    let a = parsed.symbols.iter().find(|s| s.name == "A").unwrap();
    let rel = parsed
        .relations
        .iter()
        .find(|r| r.to_name == "tag:productivity")
        .unwrap();
    assert_eq!(rel.kind, RelationKind::References);
    assert_eq!(rel.from, a.id);
}

#[test]
fn a_url_fragment_is_not_mistaken_for_a_tag() {
    let parsed = parse("# A\n\nSee http://example.com/page#section for more.\n");
    assert!(!parsed.relations.iter().any(|r| r.to_name == "tag:section"));
}

#[test]
fn hashtag_inside_the_heading_text_itself_emits_a_relation() {
    let parsed = parse("# Ideas #brainstorm\n");
    let heading = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Ideas #brainstorm")
        .unwrap();
    let rel = parsed
        .relations
        .iter()
        .find(|r| r.to_name == "tag:brainstorm")
        .unwrap();
    assert_eq!(rel.from, heading.id);
}

#[test]
fn a_wikilink_before_any_heading_is_not_indexed() {
    let parsed = parse("See [[Orphan Note]] before any heading.\n\n# A\n");
    assert!(parsed.relations.is_empty(), "{:?}", parsed.relations);
}

#[test]
fn an_all_digit_hashtag_is_not_indexed_as_a_tag() {
    let parsed = parse("# A\n\nFixes #123 and #v2 today.\n");
    assert!(!parsed.relations.iter().any(|r| r.to_name == "tag:123"));
    assert!(parsed.relations.iter().any(|r| r.to_name == "tag:v2"));
}

#[test]
fn a_wikilink_in_a_list_item_is_scanned_like_any_paragraph() {
    let parsed = parse("# A\n\n- See [[Other Note]] and #tagged\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Other Note"));
    assert!(parsed.relations.iter().any(|r| r.to_name == "tag:tagged"));
}

#[test]
fn a_wikilink_in_a_blockquote_is_scanned_like_any_paragraph() {
    let parsed = parse("# A\n\n> See [[Other Note]] and #tagged\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Other Note"));
    assert!(parsed.relations.iter().any(|r| r.to_name == "tag:tagged"));
}

#[test]
fn a_wikilink_in_a_table_cell_is_not_scanned() {
    let parsed = parse("# A\n\n| h |\n| - |\n| [[Cell]] |\n");
    assert!(parsed.relations.is_empty(), "{:?}", parsed.relations);
}
