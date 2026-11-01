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
fn require_resolves_a_package_to_its_init_file() {
    let index = open_indexed();
    assert!(import_hits(&index, "shapes").contains(&(
        Resolution::Resolved,
        Some("app/shapes/init.lua".to_string())
    )));
}

#[test]
fn dofile_and_loadfile_stay_unresolved_because_the_working_directory_is_unknown() {
    let index = open_indexed();
    // `lib/util.lua` exists, but dofile/loadfile resolve against the process
    // working directory, which nothing in the source proves. The literal is
    // recorded as written and no file is picked.
    let hits = |path: &str, kind: &str| -> Vec<Resolution> {
        index
            .find_references(path)
            .unwrap()
            .into_iter()
            .filter(|h| h.kind == kind && h.relative_path == "main.lua" && h.to_name == path)
            .map(|h| h.resolution)
            .collect()
    };
    assert_eq!(hits("lib/util.lua", "imports"), [Resolution::Unresolved]);
    assert_eq!(hits("lib/user.lua", "references"), [Resolution::Unresolved]);
}

#[test]
fn a_third_party_module_matches_nothing_local() {
    let index = open_indexed();
    assert!(import_hits(&index, "cjson")
        .iter()
        .all(|(r, _)| *r == Resolution::Unresolved));
}

#[test]
fn replacing_require_with_a_member_call_removes_the_import_on_reindex() {
    let root = std::env::temp_dir().join(format!(
        "mct-lua-reindex-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let source = root.join("main.lua");
    std::fs::write(root.join("actual.lua"), "return {}\n").unwrap();
    std::fs::write(&source, "require('actual')\nlocal low = -5\n").unwrap();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(LuaParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    assert_eq!(import_hits(&index, "actual").len(), 1);
    assert_eq!(index.find_symbol("low").unwrap().len(), 1);
    std::fs::write(&source, "loader.require('actual')\n").unwrap();
    index
        .reindex_paths(&registry, std::slice::from_ref(&source))
        .unwrap();
    assert!(import_hits(&index, "actual").is_empty());
    assert!(index.find_symbol("low").unwrap().is_empty());
    std::fs::remove_file(&source).unwrap();
    index.reindex_paths(&registry, &[source]).unwrap();
    assert!(index.find_calls("main").unwrap().is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
