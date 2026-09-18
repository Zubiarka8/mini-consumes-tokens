//! Runs the real `mct-index` pipeline against a genuine small multi-file C++
//! project split the idiomatic C++ way — `Invoice.h` declares `addItem`
//! (both overloads), `Invoice.cpp` defines them — exercising find_symbol/
//! find_references/find_calls/find_callers, including the
//! declaration/definition correlation this crate is built around (see
//! `src/lib.rs`'s module doc).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-cpp/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_cpp::CppParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/billing-app")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(CppParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 5, "Invoice.h/.cpp, Logger.h/.cpp, Main.cpp");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn find_symbol_correlates_header_declarations_with_source_definitions() {
    let index = open_indexed();
    let hits = index.find_symbol("addItem").unwrap();
    let methods: Vec<_> = hits.iter().filter(|h| h.kind == "method").collect();
    // 2 overloads declared in Invoice.h + the same 2 defined in Invoice.cpp.
    assert_eq!(methods.len(), 4, "both overloads' declaration and definition should all be indexed: {hits:?}");
    assert!(methods.iter().all(|h| h.parent.as_deref() == Some("Invoice")), "declaration and definition must share the same parent to correlate as one logical member");

    let declared: Vec<_> = methods.iter().filter(|h| h.relative_path == "Invoice.h").collect();
    let defined: Vec<_> = methods.iter().filter(|h| h.relative_path == "Invoice.cpp").collect();
    assert_eq!(declared.len(), 2, "both overloads declared in the header");
    assert_eq!(defined.len(), 2, "both overloads defined in the source file");
}

#[test]
fn find_calls_reports_log_only_from_the_definitions_with_bodies() {
    let index = open_indexed();
    let calls = index.find_calls("addItem").unwrap();
    let log_calls: Vec<_> = calls.iter().filter(|c| c.to_name == "log").collect();
    // The header declarations have no body, so only the two definitions in
    // Invoice.cpp actually call Logger::log.
    assert_eq!(log_calls.len(), 2, "each definition calls Logger::log once: {calls:?}");
    assert!(log_calls.iter().all(|c| c.relative_path == "Invoice.cpp"));
}

#[test]
fn find_callers_of_log_shows_add_item_as_caller() {
    let index = open_indexed();
    let callers = index.find_callers("log").unwrap();
    assert_eq!(callers.len(), 2);
    assert!(callers.iter().all(|c| c.from_symbol == "addItem"));
}

#[test]
fn find_references_finds_cross_file_calls_from_main() {
    let index = open_indexed();
    let refs = index.find_references("addItem").unwrap();
    assert_eq!(refs.len(), 2, "Main.cpp calls addItem twice");
    assert!(refs.iter().all(|r| r.relative_path == "Main.cpp"));
}
