//! A call through a name the script does not bind: a `class_name` global,
//! an autoload from `project.godot`, an engine class or an untyped value.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_gdscript::GdscriptParser;

const PLAYER: &str = "actors/player.gd";

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/global_receivers");
    mct_corpus::open_fixture_indexed(&root, Arc::new(GdscriptParser))
}

fn call(index: &Index, from: &str, name: &str) -> RelationHit {
    let mut hits: Vec<_> = index
        .find_callers(name)
        .unwrap()
        .into_iter()
        .filter(|hit| hit.relative_path == PLAYER && hit.from_symbol == from)
        .collect();
    assert_eq!(hits.len(), 1, "{from} -> {name}: {hits:?}");
    hits.remove(0)
}

#[test]
fn a_class_name_global_binds_to_its_own_method() {
    let index = open_indexed();
    let hit = call(&index, "_ready", "find");
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    assert_eq!(hit.candidates[0].relative_path, "items/item_catalog.gd");
}

#[test]
fn an_autoload_binds_to_the_script_it_instances() {
    let index = open_indexed();
    let hit = call(&index, "_ready", "start");
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    assert_eq!(hit.candidates[0].relative_path, "core/game.gd");
}

/// Known limit: the autoload `extends` the class named after its file
/// (`scores`), but this script declares `class_name HighScores`.
#[test]
fn an_autoload_whose_script_has_another_class_name_is_unresolved() {
    let index = open_indexed();
    let hit = call(&index, "_ready", "submit");
    assert_eq!(hit.resolution, Resolution::Unresolved, "{hit:?}");
}

#[test]
fn an_engine_class_does_not_bind_to_a_project_method() {
    let index = open_indexed();
    let hit = call(&index, "_ready", "is_action_pressed");
    assert_eq!(hit.resolution, Resolution::Unresolved, "{hit:?}");
}

#[test]
fn an_untyped_value_stays_ambiguous_between_same_named_methods() {
    let index = open_indexed();
    let hit = call(&index, "use", "find");
    assert_eq!(hit.resolution, Resolution::Ambiguous, "{hit:?}");
    assert_eq!(hit.candidate_count, 2, "{hit:?}");
}
