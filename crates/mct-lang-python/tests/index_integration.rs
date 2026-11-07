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

use mct_index::Index;
use mct_lang_python::PythonParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/billing-app")
}

fn open_indexed() -> Index {
    mct_corpus::open_indexed(
        &fixture_root(),
        Arc::new(PythonParser),
        3,
        "invoice.py, logger.py, main.py",
    )
}

#[test]
fn find_symbol_locates_method_on_its_class() {
    mct_corpus::assert_indexed_symbol(
        &open_indexed(),
        "add_item",
        "method",
        "Invoice",
        "invoice.py",
        "python",
    );
}

#[test]
fn find_symbol_indexes_the_class_and_the_free_function_separately() {
    mct_corpus::assert_fixture_type_and_function(
        &open_indexed(),
        "class",
        "invoice.py",
        "logger.py",
    );
}

#[test]
fn list_symbols_reports_every_definition_in_the_class_file_as_python() {
    let index = open_indexed();
    let entries = index.list_symbols("invoice.py", None, None).unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "invoice",
            "Invoice",
            "__init__",
            "add_item",
            "add_item_with_tax"
        ]
    );
    assert!(
        entries.iter().all(|e| e.language == "python"),
        "files.language must be python for every row: {entries:?}"
    );
    // The language filter is applied against that same column.
    assert_eq!(
        index
            .list_symbols("invoice.py", None, Some("python"))
            .unwrap()
            .len(),
        5
    );
    assert!(index
        .list_symbols("invoice.py", None, Some("ruby"))
        .unwrap()
        .is_empty());
}

#[test]
fn status_reports_full_python_coverage() {
    mct_corpus::assert_single_language_coverage(&open_indexed(), "python", 3, 9);
}

#[test]
fn find_calls_reports_log_from_add_item() {
    let index = open_indexed();
    mct_corpus::assert_calls_once(&index, "add_item", "log", "invoice.py");
}

#[test]
fn find_callers_of_log_shows_both_invoice_methods() {
    let index = open_indexed();
    mct_corpus::assert_callers_named(
        &index,
        "log",
        "invoice.py",
        &["add_item", "add_item_with_tax"],
    );
}

#[test]
fn find_references_finds_cross_file_call_from_main() {
    mct_corpus::assert_fixture_main_reference(&open_indexed(), "main.py");
}

#[test]
fn find_references_of_log_includes_the_from_import_as_well_as_the_calls() {
    mct_corpus::assert_fixture_log_references(&open_indexed(), "invoice.py");
}
