//! Runs the real `mct-index` pipeline against a small multi-file Go package:
//! a struct with pointer-receiver methods (`invoice.go`), a free function
//! it calls (`logger.go`), an interface it does NOT declare implementing —
//! Go has no such keyword (`shape.go`) — and a caller (`runner.go`).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-go/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_go::GoParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/billing-app")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(GoParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 4, "invoice.go, logger.go, shape.go, runner.go");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn find_symbol_locates_receiver_method_on_its_struct() {
    let index = open_indexed();
    let hits = index.find_symbol("AddItem").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "method");
    assert_eq!(hits[0].parent.as_deref(), Some("Invoice"));
    assert_eq!(hits[0].relative_path, "invoice.go");
    // `language` is read straight off the `files.language` column via the
    // symbols->files JOIN, so this is what proves the row landed under the
    // right language and not merely that the parser produced the symbol.
    assert_eq!(hits[0].language, "go");
}

#[test]
fn find_symbol_indexes_interface_and_its_declared_methods() {
    let index = open_indexed();
    let shape = index.find_symbol("Shape").unwrap();
    assert_eq!(shape.len(), 1);
    assert_eq!(shape[0].kind, "interface");

    let area = index.find_symbol("Area").unwrap();
    assert_eq!(area.len(), 1);
    assert_eq!(area[0].kind, "method");
    assert_eq!(area[0].parent.as_deref(), Some("Shape"));
}

#[test]
fn find_calls_reports_log_from_add_item() {
    let index = open_indexed();
    let calls = index.find_calls("AddItem").unwrap();
    let log_calls: Vec<_> = calls.iter().filter(|c| c.to_name == "Log").collect();
    assert_eq!(log_calls.len(), 1);
    assert_eq!(log_calls[0].relative_path, "invoice.go");
}

#[test]
fn find_callers_of_log_shows_both_invoice_methods() {
    let index = open_indexed();
    let callers = index.find_callers("Log").unwrap();
    assert_eq!(callers.len(), 2, "AddItem and AddItemWithTax both call Log: {callers:?}");
    assert!(callers.iter().all(|c| c.relative_path == "invoice.go"));
}

#[test]
fn find_references_finds_cross_file_call_from_runner() {
    let index = open_indexed();
    let refs = index.find_references("AddItem").unwrap();
    assert_eq!(refs.len(), 1, "runner.go calls AddItem once");
    assert_eq!(refs[0].relative_path, "runner.go");
}
