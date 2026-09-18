//! Runs the real `mct-index` pipeline against a genuine small multi-file
//! JS/TS project mixing ES modules and CommonJS in the same fixture — a
//! CommonJS module (`mathUtils.js`), a plain ES module (`logger.ts`), a file
//! mixing both module systems and a class implementing a TS interface
//! (`invoice.ts`), and a `.tsx` component with embedded JSX calling into it
//! (`App.tsx`) — exercising find_symbol/find_references/find_calls/
//! find_callers across all four.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-js-ts/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_js_ts::JsTsParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/webapp")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(JsTsParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 4);
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn find_symbol_locates_the_invoice_class() {
    let index = open_indexed();
    let hits = index.find_symbol("Invoice").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].relative_path, "invoice.ts");
    assert_eq!(hits[0].kind, "class");
}

#[test]
fn find_calls_reports_add_item_calling_compute_total_and_log() {
    let index = open_indexed();
    let calls = index.find_calls("addItem").unwrap();
    let names: Vec<_> = calls.iter().map(|c| c.to_name.as_str()).collect();
    assert!(names.contains(&"computeTotal"), "calls from addItem: {names:?}");
    assert!(names.contains(&"log"), "calls from addItem: {names:?}");
}

#[test]
fn find_callers_of_add_item_shows_cross_file_caller_in_app_tsx() {
    let index = open_indexed();
    let callers = index.find_callers("addItem").unwrap();
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0].from_symbol, "useInvoiceTotal");
    assert_eq!(callers[0].relative_path, "App.tsx");
}

#[test]
fn find_callers_of_use_invoice_total_shows_both_call_sites_in_app() {
    let index = open_indexed();
    let callers = index.find_callers("useInvoiceTotal").unwrap();
    assert_eq!(callers.len(), 2, "one call in App's body, one inside the JSX onClick handler: {callers:?}");
    assert!(callers.iter().all(|c| c.from_symbol == "App" && c.relative_path == "App.tsx"));
}

#[test]
fn find_references_finds_the_commonjs_export_and_the_real_call_of_add() {
    let index = open_indexed();
    let refs = index.find_references("add").unwrap();
    let kinds: Vec<_> = refs.iter().map(|r| r.kind.as_str()).collect();
    assert!(kinds.contains(&"references"), "mathUtils.js's `module.exports = {{ add }}` should show up: {kinds:?}");
    assert!(kinds.contains(&"calls"), "invoice.ts's computeTotal calling add() should show up: {kinds:?}");
}

#[test]
fn invoice_implements_priced_interface() {
    let index = open_indexed();
    let refs = index.find_references("Priced").unwrap();
    assert!(refs.iter().any(|r| r.kind == "implements" && r.from_symbol == "Invoice"));
}

#[test]
fn require_of_math_utils_is_recorded_as_an_import() {
    let index = open_indexed();
    let refs = index.find_references("./mathUtils").unwrap();
    assert!(refs.iter().any(|r| r.kind == "imports" && r.relative_path == "invoice.ts"));
}
