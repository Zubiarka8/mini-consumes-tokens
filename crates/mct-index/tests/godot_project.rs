//! `project.godot` is both a manifest (enabled editor plugins) and a source
//! of symbols (singleton autoloads). These tests pin that both kinds of rows
//! follow the file through edits, invalid UTF-8 and deletion.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

#[path = "common/scratch_dir.rs"]
mod scratch_dir;
use scratch_dir::scratch_dir;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, UnsupportedKind};

const PROJECT: &str = concat!(
    "config_version=5\n",
    "[autoload]\n",
    "Game=\"*res://game.gd\"\n",
    "[editor_plugins]\n",
    "enabled=PackedStringArray(\"res://addons/tiles/plugin.cfg\")\n",
);

fn plugins(index: &Index) -> Vec<String> {
    index
        .status()
        .unwrap()
        .dependencies
        .into_iter()
        .filter(|manifest| manifest.manifest_path == "project.godot")
        .flat_map(|manifest| manifest.dependencies)
        .map(|dep| dep.name)
        .collect()
}

#[test]
fn invalid_utf8_keeps_the_last_good_rows_and_is_a_read_failure() {
    let root = scratch_dir("mct-godot-utf8");
    let manifest = root.join("project.godot");
    fs::write(&manifest, PROJECT).unwrap();
    let registry = LanguageRegistry::new();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    assert_eq!(index.find_symbol("Game").unwrap().len(), 1);
    assert_eq!(plugins(&index), ["res://addons/tiles/plugin.cfg"]);

    // An editor crash left a truncated multi-byte character behind.
    let mut broken = PROJECT.replace("Game=", "Gamé=").into_bytes();
    let accent = broken.iter().position(|&b| b == 0xC3).unwrap();
    broken.remove(accent + 1);
    fs::write(&manifest, &broken).unwrap();
    index
        .reindex_paths(&registry, std::slice::from_ref(&manifest))
        .unwrap();

    assert_eq!(index.find_symbol("Game").unwrap().len(), 1, "symbols stay");
    assert_eq!(
        plugins(&index),
        ["res://addons/tiles/plugin.cfg"],
        "dependencies stay"
    );
    let status = index.status().unwrap();
    assert_eq!(status.read_failures.len(), 1, "{status:?}");
    assert_eq!(status.read_failures[0].relative_path, "project.godot");
    assert_eq!(status.read_failures[0].kind, UnsupportedKind::ReadFailure);

    // Fixed: the new rows replace the old ones and the failure clears.
    fs::write(&manifest, PROJECT.replace("Game=", "World=")).unwrap();
    index.reindex(&registry, false).unwrap();
    assert!(index.status().unwrap().read_failures.is_empty());
    assert!(index.find_symbol("Game").unwrap().is_empty());
    assert_eq!(index.find_symbol("World").unwrap().len(), 1);

    drop(index);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn deleting_project_godot_drops_its_symbols_and_dependencies() {
    let root = scratch_dir("mct-godot-delete");
    let manifest = root.join("project.godot");
    fs::write(&manifest, PROJECT).unwrap();
    let registry = LanguageRegistry::new();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    assert_eq!(index.find_symbol("Game").unwrap().len(), 1);

    fs::remove_file(&manifest).unwrap();
    index.reindex_paths(&registry, &[manifest]).unwrap();
    assert!(index.find_symbol("Game").unwrap().is_empty());
    assert!(plugins(&index).is_empty());

    drop(index);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_unchanged_project_godot_is_not_rewritten() {
    let root = scratch_dir("mct-godot-unchanged");
    fs::write(root.join("project.godot"), PROJECT).unwrap();
    let registry = LanguageRegistry::new();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let first = index.reindex(&registry, false).unwrap();
    assert_eq!(first.files_parsed, 1);
    let second = index.reindex(&registry, false).unwrap();
    assert_eq!(second.files_parsed, 0, "{second:?}");
    assert_eq!(index.find_symbol("Game").unwrap().len(), 1);

    drop(index);
    fs::remove_dir_all(root).unwrap();
}
