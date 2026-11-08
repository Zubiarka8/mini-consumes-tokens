//! Runs the real `mct-index` pipeline against a small multi-file Rust crate:
//! a struct with inherent `impl` methods (`invoice.rs`), a free function it
//! `use`s and calls (`logger.rs`), and a caller (`main.rs`). Proves the whole
//! parse -> SQLite -> query round trip, not just parsing: the symbols land in
//! the index, `files.language` carries `rust`, and the query layer returns
//! the expected rows.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-rust/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_index::Index;
use mct_lang_rust::RustParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/billing-app")
}

fn open_indexed() -> Index {
    mct_corpus::open_indexed(
        &fixture_root(),
        Arc::new(RustParser),
        3,
        "invoice.rs, logger.rs, main.rs",
    )
}

#[test]
fn find_symbol_locates_inherent_method_on_its_struct() {
    let index = open_indexed();
    let hits = index.find_symbol("add_item").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "method");
    assert_eq!(
        hits[0].parent.as_deref(),
        Some("Invoice"),
        "impl-block methods hang off the type"
    );
    assert_eq!(hits[0].relative_path, "invoice.rs");
    assert_eq!(hits[0].language, "rust");
}

#[test]
fn find_symbol_indexes_the_struct_and_the_free_function_separately() {
    mct_corpus::assert_fixture_type_and_function(
        &open_indexed(),
        "struct",
        "invoice.rs",
        "logger.rs",
    );
}

#[test]
fn list_symbols_reports_every_definition_in_the_struct_file_as_rust() {
    let index = open_indexed();
    let entries = index.list_symbols("invoice.rs", None, None).unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "invoice",
            "Invoice",
            "total",
            "new",
            "add_item",
            "price",
            "add_item_with_tax",
            "price",
            "tax_rate",
            "taxed"
        ]
    );
    assert!(
        entries.iter().all(|e| e.language == "rust"),
        "files.language must be rust for every row: {entries:?}"
    );

    let methods = index
        .list_symbols("invoice.rs", Some("method"), Some("rust"))
        .unwrap();
    assert_eq!(methods.len(), 3, "new, add_item, add_item_with_tax");
    assert!(methods
        .iter()
        .all(|m| m.parent.as_deref() == Some("Invoice")));
    assert!(index
        .list_symbols("invoice.rs", None, Some("go"))
        .unwrap()
        .is_empty());
}

#[test]
fn status_reports_full_rust_coverage() {
    mct_corpus::assert_single_language_coverage(&open_indexed(), "rust", 3, 18);
}

#[test]
fn find_calls_reports_log_from_add_item() {
    let index = open_indexed();
    mct_corpus::assert_calls_once(&index, "add_item", "log", "invoice.rs");
}

#[test]
fn find_callers_of_log_shows_both_invoice_methods() {
    let index = open_indexed();
    mct_corpus::assert_callers_named(
        &index,
        "log",
        "invoice.rs",
        &["add_item", "add_item_with_tax"],
    );
}

#[test]
fn find_references_finds_cross_file_call_from_main() {
    mct_corpus::assert_fixture_main_reference(&open_indexed(), "main.rs");
}

#[test]
fn find_references_of_log_includes_the_use_import_as_well_as_the_calls() {
    mct_corpus::assert_fixture_log_references(&open_indexed(), "invoice.rs");
}

#[test]
fn find_references_of_the_struct_records_the_use_import_and_its_type_uses() {
    let index = open_indexed();
    let refs = index.find_references("Invoice").unwrap();
    let found: Vec<_> = refs
        .iter()
        .map(|r| {
            (
                r.kind.as_str(),
                r.relative_path.as_str(),
                r.from_symbol.as_str(),
            )
        })
        .collect();
    // `use invoice::Invoice;`, the `Invoice { .. }` literal in `new` and the
    // `Invoice::new()` path in `main`.
    assert_eq!(
        found,
        vec![
            ("references", "invoice.rs", "new"),
            ("imports", "main.rs", "main"),
            ("references", "main.rs", "main"),
        ],
        "{refs:?}"
    );
}
