//! `project.godot` through the GDScript parser: singleton autoloads are
//! classes that import and extend their script.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::parse;
use mct_core::{LanguageParser, ParsedFile, RelationKind, SymbolKind};
use mct_lang_gdscript::GdscriptParser;

/// `(autoload, line, kind, target, path)` for every relation.
fn rows(parsed: &ParsedFile) -> Vec<(&str, u32, RelationKind, &str, Option<&str>)> {
    parsed
        .relations
        .iter()
        .zip(&parsed.relation_targets)
        .map(|(relation, target)| {
            let symbol = &parsed.symbols[relation.from as usize];
            (
                symbol.name.as_str(),
                symbol.location.line,
                relation.kind,
                relation.to_name.as_str(),
                target.path.as_deref(),
            )
        })
        .collect()
}

#[test]
fn the_parser_claims_project_godot_and_roots_res_paths_at_it() {
    assert!(GdscriptParser.file_extensions().contains(&"godot"));
    let rooted = GdscriptParser.rooted_paths().unwrap();
    assert_eq!((rooted.scheme, rooted.marker), ("res://", "project.godot"));
}

#[test]
fn singleton_autoloads_are_classes_that_import_and_extend_their_script() {
    let parsed = parse(
        "client/project.godot",
        concat!(
            "config_version=5\n",
            "[autoload]\n",
            "Game=\"*res://core/game_state.gd\"\n",
            "Music=\"*res://audio/music.tscn\"\n",
            "[input]\n",
            "Late=\"*res://late.gd\"\n",
        ),
    );
    let names: Vec<_> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();
    // A scene autoload is a global name without a script.
    assert_eq!(names, ["Game", "Music"]);
    assert!(parsed.symbols.iter().all(|s| s.kind == SymbolKind::Class));
    assert_eq!(
        rows(&parsed),
        [
            (
                "Game",
                3,
                RelationKind::Imports,
                "game_state.gd",
                Some("res://core/game_state.gd")
            ),
            (
                "Game",
                3,
                RelationKind::Extends,
                "game_state",
                Some("res://core/game_state.gd")
            ),
        ]
    );
    assert_eq!(parsed.relation_targets[1].kind, Some(SymbolKind::Class));
}

#[test]
fn autoloads_without_a_global_name_or_with_unsafe_paths_are_skipped() {
    let parsed = parse(
        "project.godot",
        concat!(
            "[autoload]\n",
            "NotGlobal=\"res://plain.gd\"\n",
            "Bad-Name=\"*res://bad.gd\"\n",
            "=\"*res://nameless.gd\"\n",
            "Escape=\"*res://../outside.gd\"\n",
            "Dot=\"*res://./here.gd\"\n",
            "Double=\"*res://a//b.gd\"\n",
            "Empty=\"*res://\"\n",
            "Unquoted=*res://raw.gd\n",
            "Escaped=\"*res://a\\\"b.gd\"\n",
            "Elsewhere=\"*user://save.gd\"\n",
        ),
    );
    assert!(parsed.symbols.is_empty(), "{:#?}", parsed.symbols);
}

#[test]
fn a_project_file_without_autoloads_declares_nothing() {
    let parsed = parse("project.godot", "config_version=5\n[application]\n");
    assert!(parsed.symbols.is_empty());
    assert!(parsed.relations.is_empty());
}
