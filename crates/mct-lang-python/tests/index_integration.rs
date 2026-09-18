//! Runs the real `mct-index` pipeline against a small multi-file Python
//! package: a class with methods (`invoice.py`), a module-level function it
//! imports and calls (`logger.py`), and a caller (`main.py`). Proves the
//! whole parse -> SQLite -> query round trip, not just parsing: the symbols
//! land in the index, `files.language` carries `python`, and the query layer
//! returns the expected rows.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-python/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_python::PythonParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/billing-app")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(PythonParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 3, "invoice.py, logger.py, main.py");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn find_symbol_locates_method_on_its_class() {
    let index = open_indexed();
    let hits = index.find_symbol("add_item").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "method");
    assert_eq!(hits[0].parent.as_deref(), Some("Invoice"));
    assert_eq!(hits[0].relative_path, "invoice.py");
    assert_eq!(hits[0].language, "python");
}

#[test]
fn find_symbol_indexes_the_class_and_the_free_function_separately() {
    let index = open_indexed();

    let invoice = index.find_symbol("Invoice").unwrap();
    assert_eq!(invoice.len(), 1);
    assert_eq!(invoice[0].kind, "class");
    assert_eq!(invoice[0].relative_path, "invoice.py");

    let log = index.find_symbol("log").unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].kind, "function");
    assert_eq!(log[0].parent, None, "a module-level def has no enclosing class");
    assert_eq!(log[0].relative_path, "logger.py");
}

#[test]
fn list_symbols_reports_every_definition_in_the_class_file_as_python() {
    let index = open_indexed();
    let entries = index.list_symbols("invoice.py", None, None).unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["invoice", "Invoice", "__init__", "add_item", "add_item_with_tax"]);
    assert!(
        entries.iter().all(|e| e.language == "python"),
        "files.language must be python for every row: {entries:?}"
    );
    // The language filter is applied against that same column.
    assert_eq!(index.list_symbols("invoice.py", None, Some("python")).unwrap().len(), 5);
    assert!(index.list_symbols("invoice.py", None, Some("ruby")).unwrap().is_empty());
}

#[test]
fn status_reports_full_python_coverage() {
    let index = open_indexed();
    let status = index.status().unwrap();
    let python = status.languages.iter().find(|l| l.language == "python").unwrap();
    assert_eq!(python.file_count, 3);
    assert_eq!(python.symbol_count, 9);
    assert_eq!(status.languages.len(), 1, "the fixture is single-language");
    assert!(status.unsupported_languages.is_empty());
    assert!(status.syntax_errors.is_empty());
}

#[test]
fn find_calls_reports_log_from_add_item() {
    let index = open_indexed();
    let calls = index.find_calls("add_item").unwrap();
    let log_calls: Vec<_> = calls.iter().filter(|c| c.to_name == "log").collect();
    assert_eq!(log_calls.len(), 1);
    assert_eq!(log_calls[0].relative_path, "invoice.py");
}

#[test]
fn find_callers_of_log_shows_both_invoice_methods() {
    let index = open_indexed();
    let callers = index.find_callers("log").unwrap();
    assert_eq!(callers.len(), 2, "add_item and add_item_with_tax both call log: {callers:?}");
    assert!(callers.iter().all(|c| c.relative_path == "invoice.py"));
    assert!(callers.iter().any(|c| c.from_symbol == "add_item"));
    assert!(callers.iter().any(|c| c.from_symbol == "add_item_with_tax"));
}

#[test]
fn find_references_finds_cross_file_call_from_main() {
    let index = open_indexed();
    let refs = index.find_references("add_item").unwrap();
    assert_eq!(refs.len(), 1, "main.py calls add_item once");
    assert_eq!(refs[0].relative_path, "main.py");
    assert_eq!(refs[0].kind, "calls");
    assert_eq!(refs[0].from_symbol, "main");
}

#[test]
fn find_references_of_log_includes_the_from_import_as_well_as_the_calls() {
    let index = open_indexed();
    let refs = index.find_references("log").unwrap();
    assert_eq!(refs.len(), 3, "one `from logger import log` plus two calls: {refs:?}");
    assert_eq!(refs.iter().filter(|r| r.kind == "imports").count(), 1);
    assert_eq!(refs.iter().filter(|r| r.kind == "calls").count(), 2);
    assert!(refs.iter().all(|r| r.relative_path == "invoice.py"));
}
