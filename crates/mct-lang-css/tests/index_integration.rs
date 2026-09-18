//! Runs the real `mct-index` pipeline against a small 2-file stylesheet:
//! `main.css` (an `@import`, a simple id selector, two comma-separated class
//! selectors, one nested inside a `@media` block) and `extra.css` (the
//! imported file, with its own unrelated rule).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-css/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_css::CssParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/theme")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(CssParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 2, "main.css, extra.css");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn find_symbol_locates_the_id_selector() {
    let index = open_indexed();
    let hits = index.find_symbol("#header").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "rule");
    assert_eq!(hits[0].relative_path, "main.css");
}

#[test]
fn find_symbol_locates_the_comma_separated_class_selector_with_one_use() {
    let index = open_indexed();
    assert_eq!(index.find_symbol(".sidebar").unwrap().len(), 1);
}

#[test]
fn find_symbol_locates_the_rule_nested_in_a_media_query() {
    let index = open_indexed();
    // ".nav" appears twice: once in the top-level comma list, once again
    // inside @media — two distinct Rule symbols, same name.
    let hits = index.find_symbol(".nav").unwrap();
    assert_eq!(hits.len(), 2, "top-level .nav and the one nested in @media: {hits:?}");
}

#[test]
fn find_references_shows_the_cross_file_import() {
    let index = open_indexed();
    let refs = index.find_references("extra.css").unwrap();
    assert_eq!(refs.len(), 1, "only main.css imports extra.css");
    assert_eq!(refs[0].relative_path, "main.css");
    assert_eq!(refs[0].kind, "imports");
}

#[test]
fn status_reports_full_css_coverage() {
    let index = open_indexed();
    let status = index.status().unwrap();
    let css = status.languages.iter().find(|l| l.language == "css").unwrap();
    assert_eq!(css.file_count, 2);
    assert!(status.unsupported_languages.is_empty());
    assert!(status.syntax_errors.is_empty());
}
