//! File-level `Imports` evidence: each cross-file `use`/`mod` names its
//! target and carries enough evidence to resolve to the right file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::{LanguageParser, LanguageRegistry, RelationKind, SourceFile, SymbolKind};
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_rust::RustParser;

fn parse(path: &str, src: &str) -> mct_core::ParsedFile {
    RustParser
        .parse(&SourceFile {
            relative_path: path.to_string(),
            contents: src.to_string(),
        })
        .unwrap()
}

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(RustParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    index
}

/// What the `imports` relation named `to` from `from_file` resolved to.
fn import_hit(index: &Index, to: &str, from_file: &str) -> (Resolution, Option<String>) {
    let hit = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "imports" && h.relative_path == from_file && h.to_name == to)
        .unwrap_or_else(|| panic!("no import {to} in {from_file}"));
    let file = hit.candidates.first().map(|c| c.relative_path.clone());
    (hit.resolution, file)
}

#[test]
fn mod_declarations_and_use_paths_resolve_to_the_target_file() {
    let index = open_indexed();
    // `mod util;` and `mod config;` pull in the sibling files.
    assert_eq!(
        import_hit(&index, "util", "src/lib.rs"),
        (Resolution::Resolved, Some("src/util.rs".to_string()))
    );
    assert_eq!(
        import_hit(&index, "config", "src/lib.rs"),
        (Resolution::Resolved, Some("src/config.rs".to_string()))
    );
    // `use crate::util::helper` must not pick `other.rs`'s `helper`.
    assert_eq!(
        import_hit(&index, "helper", "src/lib.rs"),
        (Resolution::Resolved, Some("src/util.rs".to_string()))
    );
    assert_eq!(
        import_hit(&index, "Settings", "src/util.rs"),
        (Resolution::Resolved, Some("src/config.rs".to_string()))
    );
}

#[test]
fn third_party_and_std_uses_never_resolve_to_a_local_namesake() {
    let index = open_indexed();
    // `other.rs` defines a `Serialize`, but `use serde::Serialize` names
    // the `serde` module.
    let (resolution, _) = import_hit(&index, "Serialize", "src/lib.rs");
    assert_ne!(resolution, Resolution::Resolved);
    let (resolution, _) = import_hit(&index, "Display", "src/lib.rs");
    assert_eq!(resolution, Resolution::External);
}

#[test]
fn mod_declaration_carries_module_evidence() {
    let parsed = parse(
        "src/lib.rs",
        "mod util;\nmod inline { fn f() {} }\nuse a::b::C;\n",
    );
    let target = |name: &str| {
        let (i, _) = parsed
            .relations
            .iter()
            .enumerate()
            .find(|(_, r)| r.kind == RelationKind::Imports && r.to_name == name)
            .unwrap_or_else(|| panic!("no import {name}"));
        parsed
            .relation_targets
            .iter()
            .find(|t| t.relation == i)
            .cloned()
    };
    let util = target("util").unwrap();
    assert_eq!(util.module.as_deref(), Some("util"));
    assert_eq!(util.kind, Some(SymbolKind::Module));
    // An inline `mod inline { … }` imports nothing.
    assert!(!parsed
        .relations
        .iter()
        .any(|r| r.to_name == "inline" && r.kind == RelationKind::Imports));
    // `use a::b::C`: `C` is a type, so `b` is the module and `C` a plain name.
    assert_eq!(target("C").unwrap().module.as_deref(), Some("b"));
}
