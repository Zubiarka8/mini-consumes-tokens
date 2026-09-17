// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/ccm-lang-md/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ccm_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use ccm_lang_md::MarkdownParser;

fn parse(src: &str) -> ccm_core::ParsedFile {
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
        "Phase 1 emits no relations: {:?}",
        parsed.relations
    );
}

#[test]
fn no_relations_are_ever_emitted_in_phase_1() {
    let parsed = parse("# A\n\nSome text with a [link](other.md) in it.\n\n## B\n");
    assert!(parsed.relations.is_empty(), "{:?}", parsed.relations);
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
    assert!(
        !parsed
            .relations
            .iter()
            .any(|r| r.to_name.contains("click here"))
    );
}

#[test]
fn wikilink_anchor_is_indexed_verbatim_not_split() {
    let parsed = parse("# A\n\nSee [[Other Note#Some Heading]] for details.\n");
    assert!(
        parsed
            .relations
            .iter()
            .any(|r| r.to_name == "Other Note#Some Heading")
    );
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
