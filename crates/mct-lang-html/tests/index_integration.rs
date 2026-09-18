//! Runs the real `mct-index` pipeline against a small 2-file, 2-language
//! fixture (`index.html` + `style.css`) to prove the cross-language part of
//! this feature end-to-end: an id'd/class'd HTML element's `References`
//! relations actually resolve against `Rule` symbols that `mct-lang-css`
//! indexed from a *different file* — not just that both parsers run without
//! error side by side (that's what `mct-mcp-server`'s `polyglot_fixture.rs`
//! already covers for unrelated languages).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-html/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_css::CssParser;
use mct_lang_html::HtmlParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/site")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(HtmlParser));
    registry.register(Arc::new(CssParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 2, "index.html, style.css");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn status_reports_coverage_for_both_languages() {
    let index = open_indexed();
    let status = index.status().unwrap();
    assert!(status.languages.iter().any(|l| l.language == "html" && l.file_count == 1));
    assert!(status.languages.iter().any(|l| l.language == "css" && l.file_count == 1));
}

#[test]
fn find_symbol_locates_the_element_and_the_rule_in_their_own_languages() {
    let index = open_indexed();

    let header_el = index.find_symbol("header").unwrap();
    assert_eq!(header_el.len(), 1);
    assert_eq!(header_el[0].kind, "element");
    assert_eq!(header_el[0].relative_path, "index.html");

    let header_rule = index.find_symbol("#header").unwrap();
    assert_eq!(header_rule.len(), 1);
    assert_eq!(header_rule[0].kind, "rule");
    assert_eq!(header_rule[0].relative_path, "style.css");
}

#[test]
fn find_references_resolves_the_id_selector_back_to_its_html_element() {
    let index = open_indexed();
    let refs = index.find_references("#header").unwrap();
    assert_eq!(refs.len(), 1, "{refs:?}");
    assert_eq!(refs[0].relative_path, "index.html");
    assert_eq!(refs[0].from_symbol, "header");
}

#[test]
fn find_references_resolves_the_class_selector_back_to_its_html_element() {
    let index = open_indexed();
    let refs = index.find_references(".nav").unwrap();
    assert_eq!(refs.len(), 1, "{refs:?}");
    assert_eq!(refs[0].relative_path, "index.html");
    assert_eq!(refs[0].from_symbol, "header");
}

#[test]
fn find_references_shows_the_stylesheet_and_script_imports() {
    let index = open_indexed();

    let css_refs = index.find_references("style.css").unwrap();
    assert_eq!(css_refs.len(), 1);
    assert_eq!(css_refs[0].relative_path, "index.html");
    assert_eq!(css_refs[0].kind, "imports");

    let js_refs = index.find_references("app.js").unwrap();
    assert_eq!(js_refs.len(), 1);
    assert_eq!(js_refs[0].relative_path, "index.html");
    assert_eq!(js_refs[0].kind, "imports");
}

#[test]
fn nested_element_without_its_own_id_reference_still_resolves_via_parent() {
    let index = open_indexed();
    let logo = index.find_symbol("logo").unwrap();
    assert_eq!(logo.len(), 1);
    assert_eq!(logo[0].parent.as_deref(), Some("header"));
}
