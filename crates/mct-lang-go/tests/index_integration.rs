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

use mct_index::Index;
use mct_lang_go::GoParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/billing-app")
}

fn open_indexed() -> Index {
    mct_corpus::open_indexed(
        &fixture_root(),
        Arc::new(GoParser),
        4,
        "invoice.go, logger.go, shape.go, runner.go",
    )
}

#[test]
fn find_symbol_locates_receiver_method_on_its_struct() {
    mct_corpus::assert_indexed_symbol(
        &open_indexed(),
        "AddItem",
        "method",
        "Invoice",
        "invoice.go",
        "go",
    );
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
    mct_corpus::assert_calls_once(&index, "AddItem", "Log", "invoice.go");
}

#[test]
fn find_callers_of_log_shows_both_invoice_methods() {
    let index = open_indexed();
    mct_corpus::assert_callers_in_file(&index, "Log", 2, "invoice.go");
}

#[test]
fn find_references_finds_cross_file_call_from_runner() {
    mct_corpus::assert_references_in_file(&open_indexed(), "AddItem", 1, "runner.go");
}
