//! File-level `Imports` evidence: a Go import names the package directory's
//! `Module` symbol (the `package` clause) and resolves through it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::{LanguageParser, LanguageRegistry, RelationKind, SourceFile, SymbolKind};
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_go::GoParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(GoParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    index
}

fn import_hit(index: &Index, to: &str) -> (Resolution, Vec<String>) {
    let hit = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "imports" && h.relative_path == "main.go" && h.to_name == to)
        .unwrap_or_else(|| panic!("no import {to}"));
    let files = hit
        .candidates
        .iter()
        .map(|c| c.relative_path.clone())
        .collect();
    (hit.resolution, files)
}

#[test]
fn a_single_file_package_is_ambiguous_only_within_its_own_file() {
    let index = open_indexed();
    // `store.go` declares both its file module and the `store` package
    // clause: two candidates, one file. The dependency is unambiguous at
    // file level even though the index reports `ambiguous`.
    let (resolution, files) = import_hit(&index, "store");
    assert_eq!(resolution, Resolution::Ambiguous);
    assert!(files.iter().all(|f| f == "store/store.go"), "{files:?}");
}

#[test]
fn a_multi_file_package_stays_ambiguous_over_its_own_files() {
    let index = open_indexed();
    let (resolution, mut files) = import_hit(&index, "billing");
    files.sort();
    assert_eq!(resolution, Resolution::Ambiguous);
    assert_eq!(files, ["billing/invoice.go", "billing/tax.go"]);
}

#[test]
fn a_third_party_package_matches_nothing_local() {
    let index = open_indexed();
    assert_eq!(import_hit(&index, "ledger").0, Resolution::Unresolved);
}

#[test]
fn the_package_relation_carries_the_directory_name_without_a_version_suffix() {
    let parsed = GoParser
        .parse(&SourceFile {
            relative_path: "main.go".to_string(),
            contents: "package main\n\nimport \"example.com/app/store/v2\"\n".to_string(),
        })
        .unwrap();
    let (i, _) = parsed
        .relations
        .iter()
        .enumerate()
        .find(|(_, r)| r.kind == RelationKind::Imports && r.to_name == "store")
        .unwrap();
    let target = parsed
        .relation_targets
        .iter()
        .find(|t| t.relation == i)
        .unwrap();
    assert_eq!(target.module.as_deref(), Some("store"));
    assert_eq!(target.kind, Some(SymbolKind::Module));
    assert!(!target.external);
}
