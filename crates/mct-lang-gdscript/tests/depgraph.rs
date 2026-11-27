//! How a GDScript script path resolves to a file for `get_dependency_graph`
//! (ADR-006). `res://` is the directory of the nearest enclosing
//! `project.godot`; a relative path starts at the script's own directory.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_index::{Index, Resolution};
use mct_lang_gdscript::GdscriptParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    mct_corpus::open_fixture_indexed(&root, Arc::new(GdscriptParser))
}

fn imported_files(index: &Index, path: &str) -> Vec<String> {
    let graph = index.dependency_graph(path, 1, 50).unwrap();
    let mut files: Vec<_> = graph
        .edges
        .into_iter()
        .filter(|edge| edge.from == path)
        .map(|edge| edge.to)
        .collect();
    files.sort();
    files
}

#[test]
fn res_extends_preload_and_load_resolve_to_files_of_the_project() {
    let index = open_indexed();
    assert_eq!(
        imported_files(&index, "actors/player.gd"),
        ["core/base.gd", "items/shield.gd", "items/weapon.gd"]
    );
}

#[test]
fn a_missing_script_stays_unresolved() {
    let index = open_indexed();
    let hit = index
        .find_references("missing.gd")
        .unwrap()
        .into_iter()
        .find(|hit| hit.kind == "imports" && hit.relative_path == "actors/player.gd")
        .expect("the preload of a missing script is recorded");
    assert_eq!(hit.resolution, Resolution::Unresolved);
    assert!(hit.candidates.is_empty(), "{:?}", hit.candidates);
}

#[test]
fn res_is_rooted_at_the_nearest_project_godot() {
    let index = open_indexed();
    // `tools/editor` is its own Godot project, so its `res://helper.gd` is
    // `tools/editor/helper.gd`, not a file at the repository root.
    assert_eq!(
        imported_files(&index, "tools/editor/panel.gd"),
        ["tools/editor/helper.gd"]
    );
}

#[test]
fn a_singleton_autoload_imports_its_script() {
    let index = open_indexed();
    assert_eq!(imported_files(&index, "project.godot"), ["core/game.gd"]);
    let game = index.find_symbol("Game").unwrap();
    assert_eq!(game.len(), 1, "{game:?}");
    assert_eq!(game[0].relative_path, "project.godot");
    assert_eq!(game[0].kind, "class");
}
