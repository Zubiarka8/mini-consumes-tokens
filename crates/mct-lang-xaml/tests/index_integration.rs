//! Runs the real `mct-index` pipeline against a small 2-file, 2-language
//! fixture (`MainWindow.xaml` + `MainWindow.xaml.cs`) to prove the
//! cross-language part of this feature end-to-end: a XAML event-handler
//! attribute's `References` relation actually resolves against a C# method
//! that `mct-lang-csharp` indexed from a *different file* — not just that
//! both parsers run without error side by side.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-xaml/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_csharp::CSharpParser;
use mct_lang_xaml::XamlParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/app")
}

fn registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(XamlParser));
    registry.register(Arc::new(CSharpParser));
    registry
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 2, "MainWindow.xaml, MainWindow.xaml.cs");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn status_reports_coverage_for_both_languages() {
    let index = open_indexed();
    let status = index.status().unwrap();
    assert!(status.languages.iter().any(|l| l.language == "xaml" && l.file_count == 1));
    assert!(status.languages.iter().any(|l| l.language == "csharp" && l.file_count == 1));
}

#[test]
fn find_symbol_locates_the_element_and_the_method_in_their_own_languages() {
    let index = open_indexed();

    let btn = index.find_symbol("SaveBtn").unwrap();
    assert_eq!(btn.len(), 1);
    assert_eq!(btn[0].kind, "element");
    assert_eq!(btn[0].relative_path, "MainWindow.xaml");
    // `.xaml` and `.xaml.cs` are the same stem with different suffixes — the
    // `files.language` column (read here via the symbols->files JOIN) is what
    // keeps the element and the handler apart.
    assert_eq!(btn[0].language, "xaml");
    // Lines are 1-based; the Button is on line 2 of MainWindow.xaml.
    assert_eq!(btn[0].line, 2);

    let handler = index.find_symbol("SaveBtn_Click").unwrap();
    assert_eq!(handler.len(), 1);
    assert_eq!(handler[0].kind, "method");
    assert_eq!(handler[0].relative_path, "MainWindow.xaml.cs");
    assert_eq!(handler[0].language, "csharp");

    let window = index.find_symbol("MainWindow").unwrap();
    let element = window.iter().find(|h| h.kind == "element").unwrap();
    assert_eq!(element.relative_path, "MainWindow.xaml");
    assert_eq!(element.language, "xaml");
    assert_eq!(element.line, 1);
}

#[test]
fn a_second_reindex_skips_unchanged_files_and_force_reparses_them() {
    let root = fixture_root();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();

    let first = index.reindex(&registry(), false).unwrap();
    assert_eq!(first.files_parsed, 2);
    let symbols_after_first = index.status().unwrap().total_symbols;

    // Incremental: nothing on disk changed, so neither grammar re-runs.
    let second = index.reindex(&registry(), false).unwrap();
    assert_eq!(second.files_parsed, 0, "{second:?}");
    assert_eq!(second.files_unchanged, 2);

    // Forced: both files re-parse, and the cross-language handler relation
    // must be rebuilt identically rather than duplicated or dropped.
    let forced = index.reindex(&registry(), true).unwrap();
    assert_eq!(forced.files_parsed, 2, "{forced:?}");
    assert!(forced.issues.is_empty(), "{:?}", forced.issues);

    assert_eq!(
        index.status().unwrap().total_symbols,
        symbols_after_first,
        "a forced reparse must not duplicate symbols"
    );
    assert_eq!(index.find_references("SaveBtn_Click").unwrap().len(), 1);
}

#[test]
fn find_references_resolves_the_click_handler_back_to_its_xaml_element() {
    let index = open_indexed();
    let refs = index.find_references("SaveBtn_Click").unwrap();
    assert_eq!(refs.len(), 1, "{refs:?}");
    assert_eq!(refs[0].relative_path, "MainWindow.xaml");
    assert_eq!(refs[0].from_symbol, "SaveBtn");
}

#[test]
fn find_references_resolves_the_loaded_handler_owned_by_the_module_root() {
    let index = open_indexed();
    let refs = index.find_references("Window_Loaded").unwrap();
    assert_eq!(refs.len(), 1, "{refs:?}");
    assert_eq!(refs[0].relative_path, "MainWindow.xaml");
    assert_eq!(refs[0].from_symbol, "MainWindow", "Window's own x:Name owns its Loaded handler");
}
