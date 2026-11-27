#![allow(clippy::unwrap_used)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind, TokenSource};
use mct_lang_gdscript::{GdscriptParser, GdscriptTokenSource};

const PLAYER: &str = r#"@abstract
class_name Player
extends "res://base.gd"

signal changed(amount: int)
enum State { IDLE, RUNNING = 3 }
const Helper = preload("res://helper.gd")

@export var speed: int = 4:
    get:
        return speed
    set(next_speed):
        speed = next_speed

class Inner:
    func ping():
        pass

@abstract func action() -> void

func _init(value: int = 2) -> void:
    super()
    var callback = func(): return value
    var later: Callable
    later = func(): return value
    self.move(callback() + later())

func move(value: int) -> void:
    print("Player movement started")
"#;

#[test]
fn extracts_godot_declarations_and_relations() {
    let file = GdscriptParser
        .parse(&SourceFile {
            relative_path: "player.gd".into(),
            contents: PLAYER.into(),
        })
        .unwrap();
    for (name, parent, kind) in [
        ("Player", Some("player.gd"), SymbolKind::Class),
        ("speed", Some("Player"), SymbolKind::Field),
        ("changed", Some("Player"), SymbolKind::Field),
        ("IDLE", Some("State"), SymbolKind::Constant),
        ("next_speed", Some("speed"), SymbolKind::Variable),
        ("Inner", Some("Player"), SymbolKind::Class),
        ("ping", Some("Inner"), SymbolKind::Method),
        ("action", Some("Player"), SymbolKind::Method),
        ("_init", Some("Player"), SymbolKind::Method),
        ("callback", Some("_init"), SymbolKind::Function),
        ("later", Some("_init"), SymbolKind::Function),
        ("value", Some("_init"), SymbolKind::Variable),
    ] {
        assert!(
            file.symbols
                .iter()
                .any(|s| s.name == name && s.parent.as_deref() == parent && s.kind == kind),
            "{name}: {:?}",
            file.symbols
        );
    }
    for (kind, name) in [
        (RelationKind::Extends, "base.gd"),
        (RelationKind::Imports, "helper.gd"),
        (RelationKind::Calls, "move"),
        (RelationKind::Calls, "callback"),
        (RelationKind::References, "export"),
    ] {
        assert!(
            file.relations
                .iter()
                .any(|r| r.kind == kind && r.to_name == name),
            "{kind:?} {name}: {:?}",
            file.relations
        );
    }
    assert!(file
        .literals
        .iter()
        .any(|l| l.text == "Player movement started"));
}

#[test]
fn tokenizes_callable_bodies_and_rejects_bad_source() {
    let tokens = GdscriptTokenSource.tokenize("player.gd", PLAYER).unwrap();
    assert!(tokens.error_lines.is_empty(), "{:?}", tokens.error_lines);
    assert!(tokens
        .callables
        .unwrap()
        .iter()
        .any(|span| span.name == "move"));
    assert!(matches!(
        GdscriptParser.parse(&SourceFile {
            relative_path: "broken.gd".into(),
            contents: "func broken(\n".into(),
        }),
        Err(mct_core::ParseError::Syntax { line: 1, .. })
    ));
}
