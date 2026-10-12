//! File-level `Imports` evidence: `require("a.b.c")` names the `Module` of
//! the file `a/b/c.lua`, scoped by its parent segment.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_lua::LuaParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(LuaParser));
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
        .filter(|h| h.kind == "imports" && h.relative_path == "main.lua" && h.to_name == to)
        .map(|h| {
            (
                h.resolution,
                h.candidates.first().map(|c| c.relative_path.clone()),
            )
        })
        .collect()
}

#[test]
fn require_resolves_to_the_file_under_the_named_package() {
    let index = open_indexed();
    // `lib/user.lua` shares the stem; the `models` segment picks the file.
    assert!(import_hits(&index, "user").contains(&(
        Resolution::Resolved,
        Some("app/models/user.lua".to_string())
    )));
    assert!(import_hits(&index, "util")
        .contains(&(Resolution::Resolved, Some("lib/util.lua".to_string()))));
}

#[test]
fn a_third_party_module_matches_nothing_local() {
    let index = open_indexed();
    assert!(import_hits(&index, "cjson")
        .iter()
        .all(|(r, _)| *r == Resolution::Unresolved));
}
