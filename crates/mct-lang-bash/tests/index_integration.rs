//! Runs the real `mct-index` pipeline against a small multi-file shell
//! project: `lib.sh` (a `log` helper), `deploy.sh` (`build`/`deploy`
//! functions, `source`s `lib.sh`, a top-level `VERSION` variable), and
//! `run.sh` (`source`s `deploy.sh` and calls `deploy`).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-bash/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_bash::BashParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/deploy-scripts")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(BashParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 3, "lib.sh, deploy.sh, run.sh");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn find_symbol_locates_function_and_top_level_variable() {
    let index = open_indexed();
    let build = index.find_symbol("build").unwrap();
    assert_eq!(build.len(), 1);
    assert_eq!(build[0].kind, "function");
    assert_eq!(build[0].relative_path, "deploy.sh");

    let version = index.find_symbol("VERSION").unwrap();
    assert_eq!(version.len(), 1);
    assert_eq!(version[0].kind, "variable");
}

#[test]
fn find_calls_reports_log_from_build() {
    let index = open_indexed();
    let calls = index.find_calls("build").unwrap();
    let log_calls: Vec<_> = calls.iter().filter(|c| c.to_name == "log").collect();
    assert_eq!(log_calls.len(), 1);
    assert_eq!(log_calls[0].relative_path, "deploy.sh");
}

#[test]
fn find_callers_of_log_shows_both_call_sites() {
    let index = open_indexed();
    let callers = index.find_callers("log").unwrap();
    assert_eq!(callers.len(), 2, "build and deploy both call log: {callers:?}");
    assert!(callers.iter().all(|c| c.relative_path == "deploy.sh"));
}

#[test]
fn find_references_finds_cross_file_call_from_run() {
    let index = open_indexed();
    let refs = index.find_references("deploy").unwrap();
    assert_eq!(refs.len(), 1, "run.sh calls deploy once");
    assert_eq!(refs[0].relative_path, "run.sh");
}

#[test]
fn source_relations_resolve_across_files() {
    let index = open_indexed();
    let refs = index.find_references("deploy").unwrap();
    assert!(!refs.is_empty());
    // deploy.sh sources lib.sh by its literal `source` argument text — the
    // same "as written at the call site" convention every other language's
    // Imports relation follows (e.g. Go's raw import string).
    let imports = index.find_references("./lib.sh").unwrap();
    assert!(imports.iter().any(|r| r.relative_path == "deploy.sh"));
}
