//! Runs the real `mct-index` pipeline against a genuine small multi-file
//! Java project, exercising find_symbol/find_references/find_calls/
//! find_callers — including the overloaded-method case explicitly required
//! for Java (two `addItem` overloads must stay distinct symbols, not
//! collapse into one).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-java/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_java::JavaParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/billing-app")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(JavaParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 3);
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn find_symbol_distinguishes_both_overloads_of_add_item() {
    let index = open_indexed();
    let hits = index.find_symbol("addItem").unwrap();
    assert_eq!(hits.len(), 2, "both overloads must be separate symbols");
    assert!(hits.iter().all(|h| h.relative_path == "Invoice.java"));
    assert!(hits.iter().all(|h| h.kind == "method"));
    assert_ne!(hits[0].line, hits[1].line);
    // `language` is read straight off the `files.language` column via the
    // symbols->files JOIN, so this is what proves the row landed under the
    // right language and not merely that the parser produced the symbol.
    assert!(hits.iter().all(|h| h.language == "java"), "{hits:?}");
}

#[test]
fn find_calls_reports_log_from_both_overloads() {
    let index = open_indexed();
    let calls = index.find_calls("addItem").unwrap();
    let log_calls: Vec<_> = calls.iter().filter(|c| c.to_name == "log").collect();
    assert_eq!(log_calls.len(), 2, "each overload calls Logger.log once: {calls:?}");
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
    assert_eq!(refs.len(), 2, "Main.java calls addItem twice");
    assert!(refs.iter().all(|r| r.relative_path == "Main.java"));
}
