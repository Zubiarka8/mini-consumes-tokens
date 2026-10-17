//! File-level `Imports` evidence: dot-sourcing / `Import-Module` of a
//! literal relative `.ps1`/`.psm1` path names that file's `Module` symbol,
//! pinned to its path.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_powershell::PowerShellParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/depgraph-fixture");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(PowerShellParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    index
}

fn import_hits(index: &Index, to: &str) -> Vec<(Resolution, Option<String>)> {
    index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "imports" && h.relative_path == "Deploy.ps1" && h.to_name == to)
        .map(|h| {
            (
                h.resolution,
                h.candidates.first().map(|c| c.relative_path.clone()),
            )
        })
        .collect()
}

fn resolved(file: &str) -> (Resolution, Option<String>) {
    (Resolution::Resolved, Some(file.to_string()))
}

#[test]
fn dot_sourced_and_imported_files_are_pinned_to_their_path() {
    let index = open_indexed();
    assert!(import_hits(&index, "Common").contains(&resolved("lib/Common.ps1")));
    // `other/Helpers.ps1` shares the stem; the path decides.
    assert!(import_hits(&index, "Helpers").contains(&resolved("Helpers.ps1")));
    assert!(import_hits(&index, "Tools").contains(&resolved("lib/Tools.psm1")));
}

#[test]
fn an_installed_module_name_is_not_a_file_import() {
    let index = open_indexed();
    assert!(import_hits(&index, "Pester")
        .iter()
        .all(|(r, _)| *r != Resolution::Resolved));
}
