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

use mct_index::Index;
use mct_lang_java::JavaParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/billing-app")
}

fn open_indexed() -> Index {
    mct_corpus::open_indexed(
        &fixture_root(),
        Arc::new(JavaParser),
        3,
        "billing-app fixture files",
    )
}

#[test]
fn find_symbol_distinguishes_both_overloads_of_add_item() {
    mct_corpus::assert_method_overloads(&open_indexed(), "addItem", "Invoice.java", "java");
}

#[test]
fn find_calls_reports_log_from_both_overloads() {
    mct_corpus::assert_overload_calls(&open_indexed(), "addItem", "log", 2);
}

#[test]
fn find_callers_of_log_shows_add_item_as_caller() {
    mct_corpus::assert_callers_from_symbol(&open_indexed(), "log", 2, "addItem");
}

#[test]
fn find_references_finds_cross_file_calls_from_main() {
    let index = open_indexed();
    mct_corpus::assert_references_in_file(&index, "addItem", 2, "Main.java");
}
