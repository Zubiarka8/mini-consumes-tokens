//! Text scenes (`.tscn`) and resources (`.tres`) through the GDScript
//! parser: what they load, and the signal connections made in the editor.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{parse, relation, relations};
use mct_core::{LanguageParser, RelationKind, SymbolKind};
use mct_lang_gdscript::GdscriptParser;

const SCENE: &str = concat!(
    "[gd_scene load_steps=3 format=3 uid=\"uid://abc\"]\n",
    "\n",
    "[ext_resource type=\"Script\" uid=\"uid://b1\" path=\"res://actors/hero.gd\" id=\"1_hero\"]\n",
    "[ext_resource type=\"PackedScene\" path=\"res://ui/hud.tscn\" id=\"2_hud\"]\n",
    "[ext_resource type=\"Script\" path=\"res://world/door.gd\" id=\"3_door\"]\n",
    "\n",
    "[node name=\"Hero\" type=\"CharacterBody2D\"]\n",
    "script = ExtResource(\"1_hero\")\n",
    "\n",
    "[node name=\"Door\" type=\"Area2D\" parent=\".\"]\n",
    "script = ExtResource(\"3_door\")\n",
    "\n",
    "[node name=\"Hud\" parent=\".\" instance=ExtResource(\"2_hud\")]\n",
    "\n",
    "[node name=\"Light\" type=\"PointLight2D\" parent=\"Door\"]\n",
    "\n",
    "[connection signal=\"opened\" from=\"Door\" to=\".\" method=\"on_door_opened\"]\n",
    "[connection signal=\"body_entered\" from=\"Door\" to=\"Door\" method=\"_on_body_entered\"]\n",
    "[connection signal=\"visibility_changed\" from=\"Door/Light\" to=\"Door/Light\" method=\"nobody\"]\n",
);

#[test]
fn the_parser_claims_scenes_and_resources() {
    for extension in ["tscn", "tres"] {
        assert!(GdscriptParser.file_extensions().contains(&extension));
    }
}

#[test]
fn a_scene_is_a_module_that_imports_what_it_loads() {
    let file = parse("levels/cave.tscn", SCENE);
    let names: Vec<_> = file
        .symbols
        .iter()
        .map(|s| (s.name.as_str(), s.kind))
        .collect();
    assert_eq!(names, [("cave.tscn", SymbolKind::Module)]);
    for (name, path, line) in [
        ("hero.gd", "res://actors/hero.gd", 3),
        ("hud.tscn", "res://ui/hud.tscn", 4),
        ("door.gd", "res://world/door.gd", 5),
    ] {
        let (site, target) = relation(&file, "cave.tscn", RelationKind::Imports, name);
        assert_eq!(target.path.as_deref(), Some(path));
        assert_eq!(target.kind, Some(SymbolKind::Module));
        assert_eq!(site.location.line, line);
    }
    // An attached script is a use of its class.
    for (class, path) in [
        ("hero", "res://actors/hero.gd"),
        ("door", "res://world/door.gd"),
    ] {
        let (_, target) = relation(&file, "cave.tscn", RelationKind::References, class);
        assert_eq!(target.kind, Some(SymbolKind::Class));
        assert_eq!(target.path.as_deref(), Some(path));
    }
}

#[test]
fn an_editor_connection_calls_the_method_on_the_target_node_script() {
    let file = parse("levels/cave.tscn", SCENE);
    for (method, path, line) in [
        ("on_door_opened", "res://actors/hero.gd", 17),
        ("_on_body_entered", "res://world/door.gd", 18),
    ] {
        let (site, target) = relation(&file, "cave.tscn", RelationKind::Calls, method);
        assert_eq!(target.path.as_deref(), Some(path), "{method}");
        assert_eq!(target.kind, Some(SymbolKind::Method), "{method}");
        assert_eq!(site.location.line, line, "{method}");
    }
    // The signal is declared by the emitting node's script.
    let (_, signal) = relation(&file, "cave.tscn", RelationKind::References, "opened");
    assert_eq!(signal.path.as_deref(), Some("res://world/door.gd"));
    assert_eq!(signal.kind, Some(SymbolKind::Field));
    // A node without a script has no method to call.
    assert!(relations(&file, RelationKind::Calls, "nobody").is_empty());
}

#[test]
fn a_text_resource_imports_and_uses_its_script() {
    let file = parse(
        "items/sword.tres",
        "[gd_resource type=\"Resource\" format=3]\n\n[ext_resource type=\"Script\" path=\"res://items/weapon.gd\" id=\"1\"]\n\n[resource]\nscript = ExtResource(\"1\")\ndamage = 7\n",
    );
    relation(&file, "sword.tres", RelationKind::Imports, "weapon.gd");
    let (_, class) = relation(&file, "sword.tres", RelationKind::References, "weapon");
    assert_eq!(class.path.as_deref(), Some("res://items/weapon.gd"));
}

#[test]
fn godot_3_bare_ids_are_read() {
    let file = parse(
        "old.tscn",
        "[gd_scene load_steps=2 format=2]\n\n[ext_resource path=\"res://old.gd\" type=\"Script\" id=1]\n\n[node name=\"Old\" type=\"Node\"]\nscript = ExtResource( 1 )\n\n[connection signal=\"ready\" from=\".\" to=\".\" method=\"_on_ready\"]\n",
    );
    let (_, target) = relation(&file, "old.tscn", RelationKind::Calls, "_on_ready");
    assert_eq!(target.path.as_deref(), Some("res://old.gd"));
}

#[test]
fn unsafe_or_foreign_resource_paths_are_skipped() {
    let file = parse(
        "bad.tscn",
        concat!(
            "[ext_resource type=\"Script\" path=\"res://../escape.gd\" id=\"1\"]\n",
            "[ext_resource type=\"Script\" path=\"res://a//b.gd\" id=\"2\"]\n",
            "[ext_resource type=\"Script\" path=\"user://save.gd\" id=\"3\"]\n",
            "[ext_resource type=\"Texture2D\" path=\"res://icon.png\" id=\"4\"]\n",
            "[ext_resource type=\"Script\" uid=\"uid://only\" id=\"5\"]\n",
            "not a section = at all\n",
        ),
    );
    assert!(file.relations.is_empty(), "{:#?}", file.relations);
}
