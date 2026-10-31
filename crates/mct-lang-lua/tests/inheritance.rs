//! Class bases reached through `require`, end to end through the real index:
//! the relation targets the required module's file, never a same-named module
//! in another package, and never a name taken from a local alias alone.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_lua::LuaParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/inheritance");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(LuaParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    index
}

/// `(resolution, candidate paths)` of every `kind` reference from `file`.
fn links(index: &Index, to: &str, kind: &str, file: &str) -> Vec<(Resolution, Vec<String>)> {
    index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == kind && h.relative_path == file && h.to_name == to)
        .map(|h| {
            (
                h.resolution,
                h.candidates
                    .iter()
                    .map(|c| c.relative_path.clone())
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn extends_resolves_to_the_required_modules_file_not_its_namesake() {
    let index = open_indexed();
    let hits = links(&index, "base", "extends", "app/child.lua");
    assert_eq!(
        hits,
        vec![(Resolution::Resolved, vec!["core/base.lua".to_string()])]
    );
}

#[test]
fn a_local_alias_is_not_the_base_name() {
    let index = open_indexed();
    // The old behavior: an Extends named after the alias `Base`.
    assert!(links(&index, "Base", "extends", "app/child.lua").is_empty());
}

#[test]
fn a_base_defined_in_the_same_file_stays_a_plain_local_name() {
    let index = open_indexed();
    let hits = links(&index, "Base", "extends", "app/unrelated.lua");
    assert_eq!(hits.len(), 1, "{hits:?}");
}

#[test]
fn a_functional_index_is_a_reference_not_an_extension() {
    let index = open_indexed();
    assert!(links(&index, "base", "extends", "app/proxy.lua").is_empty());
    assert_eq!(
        links(&index, "base", "references", "app/proxy.lua"),
        vec![(Resolution::Resolved, vec!["core/base.lua".to_string()])]
    );
}
