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
    assert_eq!(parsed.symbols.len(), 7, "six headings plus the note");
}

#[test]
fn document_without_headings_produces_only_the_note_symbol() {
    let parsed = parse("Just a paragraph of text.\n\nAnother paragraph, still no headings.\n");
    assert_eq!(parsed.symbols.len(), 1, "{:?}", parsed.symbols);
    assert_eq!(parsed.symbols[0].name, "doc");
    assert_eq!(parsed.symbols[0].kind, SymbolKind::Module);
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
fn a_wikilink_before_any_heading_belongs_to_the_note() {
    let parsed = parse("See [[Orphan Note]] before any heading.\n\n# A\n");
    assert_eq!(parsed.relations.len(), 1, "{:?}", parsed.relations);
    assert_eq!(parsed.relations[0].to_name, "Orphan Note");
    assert_eq!(parsed.relations[0].from, 0, "the note symbol owns it");
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

fn relation<'a>(
    parsed: &'a mct_core::ParsedFile,
    to_name: &str,
) -> (usize, &'a mct_core::SymbolRelation) {
    parsed
        .relations
        .iter()
        .enumerate()
        .find(|(_, r)| r.to_name == to_name)
        .unwrap_or_else(|| panic!("no relation to {to_name}: {:?}", parsed.relations))
}

fn target_of(parsed: &mct_core::ParsedFile, index: usize) -> &mct_core::RelationTarget {
    parsed
        .relation_targets
        .iter()
        .find(|t| t.relation == index)
        .expect("relation carries target evidence")
}

fn parse_at(path: &str, src: &str) -> mct_core::ParsedFile {
    MarkdownParser
        .parse(&SourceFile {
            relative_path: path.to_string(),
            contents: src.to_string(),
        })
        .expect("valid Markdown source should parse")
}

#[test]
fn a_note_with_only_a_link_still_has_a_note_symbol_and_the_relation() {
    let parsed = parse("[[beta]]\n");
    assert_eq!(parsed.symbols.len(), 1);
    assert_eq!(parsed.symbols[0].kind, SymbolKind::Module);
    assert_eq!(parsed.symbols[0].location.line, 1);
    assert_eq!(parsed.symbols[0].location.end_line, Some(1));
    assert_eq!(relation(&parsed, "beta").1.from, 0);
}

#[test]
fn an_empty_file_is_still_a_note() {
    let parsed = parse("");
    assert_eq!(parsed.symbols.len(), 1);
    assert_eq!(parsed.symbols[0].name, "doc");
    assert_eq!(parsed.symbols[0].location.byte_len, 0);
}

#[test]
fn the_note_is_named_after_the_file_stem_in_a_folder() {
    let parsed = parse_at("notes/Project Alpha.md", "# Title\n");
    assert_eq!(parsed.symbols[0].name, "Project Alpha");
    assert_eq!(
        parsed.symbols[1].parent, None,
        "its path scopes it to the note"
    );
}

#[test]
fn a_setext_heading_is_a_levelled_element_with_its_own_range() {
    let parsed = parse("Title\n=====\n\nbody\n\nSub\n---\n\nmore\n");
    let title = parsed.symbols.iter().find(|s| s.name == "Title").unwrap();
    let sub = parsed.symbols.iter().find(|s| s.name == "Sub").unwrap();
    assert_eq!((title.level, sub.level), (Some(1), Some(2)));
    assert_eq!(sub.parent.as_deref(), Some("Title"));
    assert_eq!(title.location.line, 1);
    assert_eq!(
        title.location.end_line,
        Some(9),
        "includes the nested section"
    );
    assert_eq!((sub.location.line, sub.location.end_line), (6, Some(9)));
}

#[test]
fn a_section_ends_before_the_next_equal_or_shallower_heading() {
    let parsed = parse("# A\n\na\n\n## B\n\nb\n\n## C\n\nc\n\n# D\n\nd");
    let range = |n: &str| {
        let s = parsed.symbols.iter().find(|s| s.name == n).unwrap();
        (s.location.line, s.location.end_line.unwrap())
    };
    assert_eq!(range("A"), (1, 12));
    assert_eq!(range("B"), (5, 8));
    assert_eq!(range("C"), (9, 12));
    assert_eq!(range("D"), (13, 15), "last section, no trailing newline");
    assert_eq!(
        parsed.symbols[0].location.end_line,
        Some(15),
        "note spans the file"
    );
}

#[test]
fn a_link_before_the_first_heading_is_the_notes_and_one_under_a_heading_is_the_headings() {
    let parsed = parse("[[beta]]\n\n# A\n\n[[gamma]]\n");
    assert_eq!(relation(&parsed, "beta").1.from, 0);
    let a = parsed.symbols.iter().find(|s| s.name == "A").unwrap();
    assert_eq!(relation(&parsed, "gamma").1.from, a.id);
}

#[test]
fn inline_and_fenced_code_do_not_create_links_or_tags() {
    let parsed = parse(
        "# A\n\nUse `[[ghost]]` and ``[[ghost2]] `x` #nope`` here. Real [[real]] #yes\n\n```\n[[fenced]] #fenced\n```\n\n    [[indented]]\n",
    );
    let names: Vec<&str> = parsed
        .relations
        .iter()
        .map(|r| r.to_name.as_str())
        .collect();
    assert_eq!(names, ["real", "tag:yes"], "{names:?}");
}

#[test]
fn an_unclosed_backtick_is_literal_text() {
    let parsed = parse("# A\n\na ` b [[still]] c\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "still"));
}

#[test]
fn an_embed_is_an_imports_relation_and_a_link_is_references() {
    let parsed = parse("# A\n\n![[Note]] and [[Note]]\n");
    let kinds: Vec<RelationKind> = parsed.relations.iter().map(|r| r.kind).collect();
    assert_eq!(kinds, [RelationKind::Imports, RelationKind::References]);
}

#[test]
fn a_bare_link_targets_a_note_module_by_name_only() {
    let parsed = parse("# A\n\n[[beta]]\n");
    let (i, r) = relation(&parsed, "beta");
    assert_eq!(r.kind, RelationKind::References);
    let t = target_of(&parsed, i);
    assert_eq!(
        (t.kind, t.path.as_deref(), t.module.as_deref()),
        (Some(SymbolKind::Module), None, None)
    );
}

#[test]
fn a_path_link_targets_the_exact_note_path() {
    for (src_path, link, want) in [
        ("a.md", "[[notes/Glossary]]", "notes/Glossary.md"),
        ("a.md", "[[notes/Glossary.md]]", "notes/Glossary.md"),
        ("a.md", "[[/notes/Glossary]]", "notes/Glossary.md"),
        ("x/y/a.md", "[[./b]]", "x/y/b.md"),
        ("x/y/a.md", "[[../b]]", "x/b.md"),
        ("x/y/a.md", "[[../../top/b]]", "top/b.md"),
        ("a.md", "[[folder/./sub/../Note]]", "folder/Note.md"),
    ] {
        let parsed = parse_at(src_path, &format!("{link}\n"));
        let r = &parsed.relations[0];
        let t = target_of(&parsed, 0);
        assert_eq!(t.path.as_deref(), Some(want), "{src_path} {link}");
        assert_eq!(
            r.to_name,
            want.rsplit('/').next().unwrap().trim_end_matches(".md")
        );
    }
}

#[test]
fn a_path_escaping_the_root_never_gets_a_resolvable_target() {
    let parsed = parse_at("a.md", "[[../outside]]\n");
    let r = &parsed.relations[0];
    assert_eq!(r.to_name, "../outside", "raw spelling, matching no note");
    assert_eq!(target_of(&parsed, 0).path.as_deref(), Some("../outside"));
}

#[test]
fn an_anchor_is_scoped_to_the_named_note_not_the_whole_vault() {
    let bare = parse("# A\n\n[[beta#Shared]]\n");
    let (i, _) = relation(&bare, "Shared");
    let t = target_of(&bare, i);
    assert_eq!(
        (t.kind, t.module.as_deref(), t.path.as_deref()),
        (Some(SymbolKind::Element), Some("beta"), None)
    );

    let pathed = parse_at("a.md", "[[x/beta#Shared]]\n");
    let (i, _) = relation(&pathed, "Shared");
    assert_eq!(target_of(&pathed, i).path.as_deref(), Some("x/beta.md"));

    let same = parse_at("x/a.md", "# H\n\n[[#Shared]]\n");
    let (i, r) = relation(&same, "Shared");
    assert_eq!(r.from, 1);
    assert_eq!(target_of(&same, i).path.as_deref(), Some("x/a.md"));
    assert_eq!(
        same.relations.len(),
        1,
        "no note relation for `[[#Heading]]`"
    );
}

#[test]
fn only_the_last_anchor_segment_is_the_heading_and_block_ids_are_ignored() {
    let parsed = parse("# A\n\n[[n#Outer#Inner]] [[n#^abc123]]\n");
    assert!(parsed.relations.iter().any(|r| r.to_name == "Inner"));
    assert!(!parsed.relations.iter().any(|r| r.to_name.starts_with('^')));
}

#[test]
fn frontmatter_title_aliases_and_tags_are_read_in_every_supported_form() {
    let parsed = parse(
        "---\ntitle: \"My Note\"\naliases: [One, 'Two']\ntags:\n  - alpha\n  - '#beta/x'\n---\n\n# Heading\n\n#inline\n",
    );
    let names: Vec<&str> = parsed
        .relations
        .iter()
        .map(|r| r.to_name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "title:My Note",
            "alias:One",
            "alias:Two",
            "tag:alpha",
            "tag:beta/x",
            "tag:inline"
        ]
    );
    assert!(parsed.relations[..5].iter().all(|r| r.from == 0));
    assert_eq!(
        parsed.symbols[0].name, "doc",
        "title is metadata, never identity"
    );
}

#[test]
fn a_scalar_tags_value_splits_and_unsafe_or_numeric_tags_are_dropped() {
    let parsed = parse("---\ntags: a, #b 123 bad!tag\n---\n");
    let names: Vec<&str> = parsed
        .relations
        .iter()
        .map(|r| r.to_name.as_str())
        .collect();
    assert_eq!(names, ["tag:a", "tag:b"]);
}

#[test]
fn malformed_or_unterminated_frontmatter_keeps_the_note_and_invents_nothing() {
    let unterminated = parse("---\ntitle: x\ntags: [a]\n\n# H\n");
    assert!(unterminated.symbols.iter().any(|s| s.name == "H"));
    assert!(!unterminated
        .relations
        .iter()
        .any(|r| r.to_name.starts_with("title:")));

    let odd = parse("---\n: : [\ntags: {a: b}\nunknown: x\n  nested: [[not-a-link]]\n---\n# H\n");
    assert!(odd.symbols.iter().any(|s| s.name == "H"));
    assert!(!odd.relations.iter().any(|r| r.to_name == "not-a-link"));
}

#[test]
fn a_hash_comment_in_frontmatter_is_not_a_tag() {
    let parsed = parse("---\n# a comment #nottag\ntitle: T\n---\n\nbody\n");
    assert!(!parsed.relations.iter().any(|r| r.to_name == "tag:nottag"));
}
