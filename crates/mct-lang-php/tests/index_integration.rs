//! Runs the real `mct-index` pipeline against a small multi-file PHP
//! project: `Payable.php` (an interface), `Invoice.php` (implements it,
//! `require_once`s it, has a private `validate` helper called from `pay`),
//! and `run.php` (`require_once`s `Invoice.php`, calls `pay` on a new
//! instance from `main`).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-php/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_php::PhpParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/billing-app")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(PhpParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 3, "Payable.php, Invoice.php, run.php");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn find_symbol_locates_interface_and_class() {
    let index = open_indexed();
    // Same PSR-4 filename/symbol-name collision as `Invoice` below.
    let payable = index.find_symbol("Payable").unwrap();
    assert_eq!(payable.len(), 2, "one Module hit (filename) and one Interface hit (PSR-4 collision): {payable:?}");
    let interface_hit = payable.iter().find(|s| s.kind == "interface").unwrap();
    assert_eq!(interface_hit.relative_path, "Payable.php");

    // PSR-4 naming (the file is named after its class) means the
    // filename-derived `Module` symbol and the `Invoice` class symbol share
    // a name here — same accepted behavior as every other language plugin
    // in this workspace that derives its module symbol from the filename
    // (e.g. `mct-lang-java`'s own doc comment on the equivalent case).
    let invoice = index.find_symbol("Invoice").unwrap();
    assert_eq!(invoice.len(), 2, "one Module hit (filename) and one Class hit (PSR-4 collision): {invoice:?}");
    let class_hit = invoice.iter().find(|s| s.kind == "class").unwrap();
    assert_eq!(class_hit.relative_path, "Invoice.php");
}

#[test]
fn implements_relation_resolves_to_the_interface() {
    let index = open_indexed();
    let refs = index.find_references("Payable").unwrap();
    assert!(refs.iter().any(|r| r.relative_path == "Invoice.php"), "Invoice implements Payable: {refs:?}");
}

#[test]
fn find_calls_reports_validate_from_pay() {
    let index = open_indexed();
    let calls = index.find_calls("pay").unwrap();
    let validate_calls: Vec<_> = calls.iter().filter(|c| c.to_name == "validate").collect();
    assert_eq!(validate_calls.len(), 1);
    assert_eq!(validate_calls[0].relative_path, "Invoice.php");
}

#[test]
fn find_callers_of_pay_shows_main_from_run() {
    let index = open_indexed();
    let callers = index.find_callers("pay").unwrap();
    assert_eq!(callers.len(), 1, "only main() calls pay(): {callers:?}");
    assert_eq!(callers[0].relative_path, "run.php");
}

#[test]
fn require_once_relations_resolve_across_files() {
    let index = open_indexed();
    let refs = index.find_references("Invoice.php").unwrap();
    assert!(refs.iter().any(|r| r.relative_path == "run.php"), "run.php requires Invoice.php: {refs:?}");

    let payable_refs = index.find_references("Payable.php").unwrap();
    assert!(payable_refs.iter().any(|r| r.relative_path == "Invoice.php"), "Invoice.php requires Payable.php: {payable_refs:?}");
}
