//! File-level `Imports` evidence: dot-sourcing / `Import-Module` of a
//! literal relative `.ps1`/`.psm1` path names that file's `Module` symbol,
//! pinned to its path.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_index::{Index, Resolution};
use mct_lang_powershell::PowerShellParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/depgraph-fixture");
    mct_corpus::open_fixture_indexed(&root, Arc::new(PowerShellParser))
}

fn import_hits(index: &Index, to: &str) -> Vec<(Resolution, Option<String>)> {
    mct_corpus::import_hits_from(index, "Deploy.ps1", to)
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
