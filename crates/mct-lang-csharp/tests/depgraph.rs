//! File-level `Imports` evidence: a C# `using` names a namespace, which the
//! files declaring it hold as `Module` symbols.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_csharp::CSharpParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(CSharpParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    index
}

/// Resolution and candidate files of the `imports` relation named `to` in
/// `Program.cs`.
fn import_hit(index: &Index, to: &str) -> (Resolution, Vec<String>) {
    let hit = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "imports" && h.relative_path == "Program.cs" && h.to_name == to)
        .unwrap_or_else(|| panic!("no import {to}"));
    let mut files: Vec<String> = hit
        .candidates
        .iter()
        .map(|c| c.relative_path.clone())
        .collect();
    files.sort();
    (hit.resolution, files)
}

#[test]
fn a_namespace_declared_in_one_file_resolves_to_it() {
    let index = open_indexed();
    assert_eq!(
        import_hit(&index, "Acme.Models"),
        (Resolution::Resolved, vec!["Models/Money.cs".to_string()])
    );
}

#[test]
fn a_namespace_spanning_files_is_ambiguous_over_exactly_those_files() {
    let index = open_indexed();
    assert_eq!(
        import_hit(&index, "Acme.Util"),
        (
            Resolution::Ambiguous,
            vec!["Util/Extra.cs".to_string(), "Util/Strings.cs".to_string()]
        )
    );
}

#[test]
fn bcl_namespaces_are_external() {
    let index = open_indexed();
    assert_eq!(import_hit(&index, "System").0, Resolution::External);
    assert_eq!(
        import_hit(&index, "System.Collections.Generic").0,
        Resolution::External
    );
}

#[test]
fn an_aliased_using_targets_the_aliased_namespace() {
    let index = open_indexed();
    // `using M = Acme.Models;` reports the namespace, not the alias.
    let hits = index
        .find_references("Acme.Models")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "imports" && h.relative_path == "Program.cs")
        .count();
    assert_eq!(hits, 2);
}
