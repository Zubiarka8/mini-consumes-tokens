// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-html/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_html::HtmlParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    mct_corpus::parse_source(&HtmlParser, "index.html", "HTML", src)
}

#[test]
fn file_module_ends_on_the_last_line() {
    // A trailing newline must not push the module one line past the file.
    let module = |src: &str| mct_corpus::module_span(&parse(src));
    let page = "<html>\n  <body id=\"home\"></body>\n</html>";
    mct_corpus::assert_module_spans(module, page, "<p>hi</p>\n");
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
    mct_corpus::assert_imports(&parsed, &["theme.css"]);
}

#[test]
fn non_stylesheet_link_is_not_an_import() {
    let parsed = parse("<link rel=\"icon\" href=\"favicon.ico\">");
    assert_no_imports(&parsed);
}

#[test]
fn script_src_creates_import_relation() {
    let parsed = parse("<script src=\"app.js\"></script>");
    mct_corpus::assert_imports(&parsed, &["app.js"]);
}

#[test]
fn inline_script_without_src_creates_no_import() {
    let parsed = parse("<script>console.log('hi');</script>");
    assert_no_imports(&parsed);
}

#[test]
fn module_pseudo_symbol_owns_file_level_relations() {
    let parsed = parse("<script src=\"app.js\"></script>");
    mct_corpus::assert_module_owns_import(&parsed);
}

#[test]
fn an_element_spans_through_its_closing_tag() {
    // The whole element, children and closing tag included, so relations of
    // its descendants (a `<script src>` attributed to the nearest id'd
    // ancestor) lie inside the symbol that owns them.
    let parsed = parse(
        "<body id=\"page\">\n  <div\n    id=\"outer\"\n    class=\"nav\">\n  </div>\n  <script src=\"app.js\"></script>\n</body>\n",
    );
    let span = |name: &str| {
        let s = parsed.symbols.iter().find(|s| s.name == name).unwrap();
        (s.location.line, s.location.end_line)
    };
    assert_eq!(span("outer"), (2, Some(5)));
    assert_eq!(span("page"), (1, Some(7)));
    let import = parsed
        .relations
        .iter()
        .find(|r| r.kind == RelationKind::Imports)
        .unwrap();
    assert_eq!(import.location.line, 6);
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    mct_corpus::assert_syntax_error_reported(
        &HtmlParser,
        "broken.html",
        "<div id=\"unterminated>\u{0}<<< not html at all &^%",
    );
}

/// `id`/`class` references carry the explicit CSS-language evidence the
/// index needs to link two languages; the plain import path carries none
/// (the file-level `theme` import beside it is covered in `depgraph.rs`).
#[test]
fn selector_references_target_css_and_imports_carry_no_evidence() {
    let parsed = parse(
        "<link rel=\"stylesheet\" href=\"theme.css\"><div id=\"header\" class=\"nav\"></div>",
    );
    let evidence: Vec<(&str, Option<&str>)> = parsed
        .relations
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            if let Some(t) = target.filter(|t| t.kind.is_none()) {
                assert_eq!(t.path, None);
                assert_eq!(t.qualifier, None);
                assert!(!t.external && !t.member && t.module.is_none());
            }
            (
                r.to_name.as_str(),
                target.and_then(|t| t.language.as_deref()),
            )
        })
        .collect();
    assert_eq!(
        evidence,
        vec![
            ("theme.css", None),
            ("theme", Some("css")),
            ("#header", Some("css")),
            (".nav", Some("css")),
        ]
    );
}

fn assert_no_imports(parsed: &mct_core::ParsedFile) {
    assert!(parsed
        .relations
        .iter()
        .all(|r| r.kind != RelationKind::Imports));
}
