//! Runs the real `mct-index` pipeline against a small multi-file PowerShell
//! project: `Lib.psm1` (a `Write-Log` helper), `Deploy.ps1`
//! (`Invoke-Build`/`Invoke-Deploy` functions, imports `Lib`, a top-level
//! `$Version` variable), and `Run.ps1` (dot-sources `Deploy.ps1` and calls
//! `Invoke-Deploy`).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-powershell/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_powershell::PowerShellParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/deploy-scripts")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(PowerShellParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 3, "Lib.psm1, Deploy.ps1, Run.ps1");
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn find_symbol_locates_function_and_top_level_variable() {
    let index = open_indexed();
    let build = index.find_symbol("Invoke-Build").unwrap();
    assert_eq!(build.len(), 1);
    assert_eq!(build[0].kind, "function");
    assert_eq!(build[0].relative_path, "Deploy.ps1");
    // `language` is read straight off the `files.language` column via the
    // symbols->files JOIN — and .ps1 and .psm1 must both resolve to the one
    // `powershell` id (Lib.psm1's Write-Log is asserted below).
    assert_eq!(build[0].language, "powershell");

    let version = index.find_symbol("Version").unwrap();
    assert_eq!(version.len(), 1);
    assert_eq!(version[0].kind, "variable");
    assert_eq!(version[0].language, "powershell");

    let write_log = index.find_symbol("Write-Log").unwrap();
    assert_eq!(write_log.len(), 1);
    assert_eq!(write_log[0].relative_path, "Lib.psm1");
    assert_eq!(write_log[0].language, "powershell");
}

#[test]
fn find_calls_reports_write_log_from_invoke_build() {
    let index = open_indexed();
    let calls = index.find_calls("Invoke-Build").unwrap();
    let log_calls: Vec<_> = calls.iter().filter(|c| c.to_name == "Write-Log").collect();
    assert_eq!(log_calls.len(), 1);
    assert_eq!(log_calls[0].relative_path, "Deploy.ps1");
}

#[test]
fn find_callers_of_write_log_shows_both_call_sites() {
    let index = open_indexed();
    let callers = index.find_callers("Write-Log").unwrap();
    assert_eq!(callers.len(), 2, "Invoke-Build and Invoke-Deploy both call Write-Log: {callers:?}");
    assert!(callers.iter().all(|c| c.relative_path == "Deploy.ps1"));
}

#[test]
fn find_references_finds_cross_file_call_from_run() {
    let index = open_indexed();
    let refs = index.find_references("Invoke-Deploy").unwrap();
    assert_eq!(refs.len(), 1, "Run.ps1 calls Invoke-Deploy once");
    assert_eq!(refs[0].relative_path, "Run.ps1");
}

#[test]
fn import_module_relation_resolves_by_literal_name() {
    let index = open_indexed();
    let refs = index.find_references("Lib").unwrap();
    assert!(refs.iter().any(|r| r.relative_path == "Deploy.ps1"), "Deploy.ps1 does `Import-Module Lib`");
}
