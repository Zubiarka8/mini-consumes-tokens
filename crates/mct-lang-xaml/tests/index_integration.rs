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

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(XamlParser));
    registry.register(Arc::new(CSharpParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
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

    let handler = index.find_symbol("SaveBtn_Click").unwrap();
    assert_eq!(handler.len(), 1);
    assert_eq!(handler[0].kind, "method");
    assert_eq!(handler[0].relative_path, "MainWindow.xaml.cs");
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
