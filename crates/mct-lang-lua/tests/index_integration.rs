//! Architecture acceptance test: runs the *real* `mct-index` pipeline
//! (LanguageRegistry → reindex → find_symbol/find_references/find_calls/
//! find_callers) against a genuine small multi-file Lua project, using only
//! `LuaParser` — a language outside the project's initial 7, registered
//! nowhere but in this test. If this passes without any change to
//! `mct-core`/`mct-index`, the plugin architecture holds for a language it
//! was never designed against specifically.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-lua/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_lua::LuaParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/inventory-app")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(LuaParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 3, "all 3 fixture .lua files should parse cleanly");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn find_symbol_locates_definitions_across_files() {
    let index = open_indexed();

    let inventory = index.find_symbol("Inventory").unwrap();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].relative_path, "inventory.lua");
    assert_eq!(inventory[0].kind, "module");
    // `language` is read straight off the `files.language` column via the
    // symbols->files JOIN, so this is what proves the row landed under the
    // right language and not merely that the parser produced the symbol.
    assert_eq!(inventory[0].language, "lua");

    let log = index.find_symbol("log").unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].relative_path, "logger.lua");
    assert_eq!(log[0].kind, "method");
    assert_eq!(log[0].parent.as_deref(), Some("Logger"));
    assert_eq!(log[0].language, "lua");
}

#[test]
fn find_calls_reports_callees_including_stdlib_and_cross_module_calls() {
    let index = open_indexed();

    let calls: Vec<_> = index
        .find_calls("addItem")
        .unwrap()
        .into_iter()
        .map(|r| r.to_name)
        .collect();
    assert!(calls.contains(&"insert".to_string()), "got: {calls:?}");
    assert!(calls.contains(&"log".to_string()), "got: {calls:?}");
}

#[test]
fn find_callers_finds_both_call_sites_of_log() {
    let index = open_indexed();

    let callers: Vec<_> = index
        .find_callers("log")
        .unwrap()
        .into_iter()
        .map(|r| r.from_symbol)
        .collect();
    assert!(callers.contains(&"addItem".to_string()), "got: {callers:?}");
    assert!(callers.contains(&"removeItem".to_string()), "got: {callers:?}");
    assert!(callers.contains(&"warn".to_string()), "got: {callers:?}");
    assert_eq!(callers.len(), 3);
}

#[test]
fn find_references_includes_require_of_inventory_module() {
    let index = open_indexed();

    let refs = index.find_references("inventory").unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].relative_path, "main.lua");
    assert_eq!(refs[0].kind, "imports");
}
