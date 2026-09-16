// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/ccm-lang-html/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ccm_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use ccm_lang_html::HtmlParser;

fn parse(src: &str) -> ccm_core::ParsedFile {
    HtmlParser
        .parse(&SourceFile { relative_path: "index.html".to_string(), contents: src.to_string() })
        .expect("valid HTML source should parse")
}

#[test]
fn extracts_element_with_id() {
    let parsed = parse("<div id=\"header\"></div>");
    let header = parsed.symbols.iter().find(|s| s.name == "header").unwrap();
    assert_eq!(header.kind, SymbolKind::Element);
}

#[test]
fn element_without_id_is_not_indexed() {
    let parsed = parse("<div class=\"nav\"></div>");
    assert!(parsed.symbols.iter().all(|s| s.kind != SymbolKind::Element));
}

#[test]
fn id_and_class_attributes_produce_references_to_matching_css_selectors() {
    let parsed = parse("<div id=\"header\" class=\"nav sidebar\"></div>");
    let header = parsed.symbols.iter().find(|s| s.name == "header").unwrap();
    let refs: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.from == header.id && r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(refs.contains(&"#header"), "{refs:?}");
    assert!(refs.contains(&".nav"), "{refs:?}");
    assert!(refs.contains(&".sidebar"), "{refs:?}");
}

#[test]
fn nested_element_parent_tracks_nearest_id_ancestor() {
    let parsed = parse("<div id=\"outer\"><span id=\"inner\"></span></div>");
    let inner = parsed.symbols.iter().find(|s| s.name == "inner").unwrap();
    assert_eq!(inner.parent.as_deref(), Some("outer"));
}

#[test]
fn id_ancestor_is_found_through_an_unidentified_element() {
    // <div id="outer"> -> <p> (no id) -> <span id="inner"> : inner's nearest
    // *id'd* ancestor is still "outer", the unidentified <p> is just skipped.
    let parsed = parse("<div id=\"outer\"><p><span id=\"inner\"></span></p></div>");
    let inner = parsed.symbols.iter().find(|s| s.name == "inner").unwrap();
    assert_eq!(inner.parent.as_deref(), Some("outer"));
}

#[test]
fn link_stylesheet_creates_import_relation() {
    let parsed = parse("<link rel=\"stylesheet\" href=\"theme.css\">");
    let imports: Vec<_> =
        parsed.relations.iter().filter(|r| r.kind == RelationKind::Imports).map(|r| r.to_name.as_str()).collect();
    assert!(imports.contains(&"theme.css"), "{imports:?}");
}

#[test]
fn non_stylesheet_link_is_not_an_import() {
    let parsed = parse("<link rel=\"icon\" href=\"favicon.ico\">");
    assert!(parsed.relations.iter().all(|r| r.kind != RelationKind::Imports));
}

#[test]
fn script_src_creates_import_relation() {
    let parsed = parse("<script src=\"app.js\"></script>");
    let imports: Vec<_> =
        parsed.relations.iter().filter(|r| r.kind == RelationKind::Imports).map(|r| r.to_name.as_str()).collect();
    assert!(imports.contains(&"app.js"), "{imports:?}");
}

#[test]
fn inline_script_without_src_creates_no_import() {
    let parsed = parse("<script>console.log('hi');</script>");
    assert!(parsed.relations.iter().all(|r| r.kind != RelationKind::Imports));
}

#[test]
fn module_pseudo_symbol_owns_file_level_relations() {
    let parsed = parse("<script src=\"app.js\"></script>");
    let module = parsed.symbols.iter().find(|s| s.kind == SymbolKind::Module).unwrap();
    let import = parsed.relations.iter().find(|r| r.kind == RelationKind::Imports).unwrap();
    assert_eq!(import.from, module.id);
}

#[test]
fn element_end_line_spans_a_multiline_start_tag() {
    // An Element symbol is located at its `start_tag` node, not the whole
    // `element` (which would include children and the closing tag) — so a
    // single-line tag's `end_line` equals `line` (see the sibling
    // `id_and_class_attributes_produce_references_to_matching_css_selectors`
    // test for that case); a tag whose own attributes span multiple lines is
    // what exercises a real `end_line > line` here.
    let parsed = parse("<div\n    id=\"outer\"\n    class=\"nav\">\n</div>\n");
    let outer = parsed
        .symbols
        .iter()
        .find(|s| s.name == "outer")
        .expect("outer element should be indexed");
    assert_eq!(outer.location.line, 1);
    assert_eq!(outer.location.end_line, Some(3));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = HtmlParser.parse(&SourceFile {
        relative_path: "broken.html".to_string(),
        contents: "<div id=\"unterminated>\u{0}<<< not html at all &^%".to_string(),
    });
    assert!(matches!(result, Err(ccm_core::ParseError::Syntax { .. })), "{result:?}");
}
