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

fn registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(CssParser));
    registry
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 2, "main.css, extra.css");
    assert!(
        report.issues.is_empty(),
        "no parse issues expected: {:?}",
        report.issues
    );
    index
}

#[test]
fn find_symbol_locates_the_id_selector() {
    let index = open_indexed();
    let hits = index.find_symbol("#header").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "rule");
    assert_eq!(hits[0].relative_path, "main.css");
    // `language` is read straight off the `files.language` column via the
    // symbols->files JOIN, so this is what proves the row landed under the
    // right language and not merely that the parser produced the symbol.
    assert_eq!(hits[0].language, "css");
    // A selector's recorded position is the only thing tying an indexed rule
    // back to the source text — assert it exactly. Lines are 1-based.
    assert_eq!(hits[0].line, 3, "`#header {{` is on line 3 of main.css");
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
    assert_eq!(
        hits.len(),
        2,
        "top-level .nav and the one nested in @media: {hits:?}"
    );
    // Two same-named rules are only distinguishable by position, so the
    // lines are load-bearing here: line 7 is the comma list, line 13 the
    // copy nested inside `@media (min-width: 600px)`.
    let mut lines: Vec<_> = hits.iter().map(|h| h.line).collect();
    lines.sort_unstable();
    assert_eq!(lines, vec![7, 13], "{hits:?}");
    assert!(hits
        .iter()
        .all(|h| h.kind == "rule" && h.relative_path == "main.css"));
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
    let css = status
        .languages
        .iter()
        .find(|l| l.language == "css")
        .unwrap();
    assert_eq!(css.file_count, 2);
    assert!(status.unsupported_languages.is_empty());
    assert!(status.syntax_errors.is_empty());
}

#[test]
fn a_second_reindex_skips_unchanged_files_and_force_reparses_them() {
    let root = fixture_root();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();

    let first = index.reindex(&registry(), false).unwrap();
    assert_eq!(first.files_parsed, 2);
    let symbols_after_first = index.status().unwrap().total_symbols;

    // Incremental: nothing on disk changed, so the real grammar must not be
    // re-run at all.
    let second = index.reindex(&registry(), false).unwrap();
    assert_eq!(second.files_parsed, 0, "{second:?}");
    assert_eq!(second.files_unchanged, 2);

    // Forced: re-parses despite being unchanged, and must land on exactly
    // the same symbol set rather than duplicating rows.
    let forced = index.reindex(&registry(), true).unwrap();
    assert_eq!(forced.files_parsed, 2, "{forced:?}");
    assert!(forced.issues.is_empty(), "{:?}", forced.issues);

    assert_eq!(
        index.status().unwrap().total_symbols,
        symbols_after_first,
        "a forced reparse must not duplicate symbols"
    );
    assert_eq!(index.find_symbol(".nav").unwrap().len(), 2);
    assert_eq!(index.find_references("extra.css").unwrap().len(), 1);
}
