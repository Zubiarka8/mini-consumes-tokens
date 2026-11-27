//! Declarations the GDScript parser extracts, with their kinds, owners and
//! details. Each test pins one row of the parser checklist.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{detail, has_symbol, parse, relation, symbol};
use mct_core::{Modifier, ParameterKind, RelationKind, SymbolKind};

#[test]
fn a_script_without_class_name_is_a_class_named_after_its_file() {
    let file = parse(
        "enemies/slime_ai.gd",
        "extends Node\n\nfunc tick():\n    pass\n",
    );
    let module = symbol(&file, "slime_ai.gd", SymbolKind::Module);
    let class = symbol(&file, "slime_ai", SymbolKind::Class);
    assert_eq!(class.parent.as_deref(), Some("slime_ai.gd"));
    assert_eq!(module.parent, None);
    // An implicit script class has no global name, but a path to its file
    // can reach it, so it is not file-scoped: a bare name in another file
    // never looks up a snake_case class (see `relations.rs`).
    assert!(!file.file_scoped.contains(&class.id));
    assert!(file.symbol_aliases.is_empty(), "{:?}", file.symbol_aliases);
    assert_eq!(
        symbol(&file, "tick", SymbolKind::Method).parent.as_deref(),
        Some("slime_ai")
    );
}

#[test]
fn class_name_declares_a_global_class_spanning_the_file() {
    let file = parse(
        "player.gd",
        "class_name Player\nextends Node\n\nfunc a():\n    pass\n",
    );
    let class = symbol(&file, "Player", SymbolKind::Class);
    // A scene or `extends "player.gd"` names it by its file.
    assert_eq!(
        file.symbol_aliases
            .iter()
            .map(|a| (a.symbol, a.name.as_str()))
            .collect::<Vec<_>>(),
        [(class.id, "player")]
    );
    assert!(!file.file_scoped.contains(&class.id));
    assert_eq!(class.location.line, 1);
    assert_eq!(class.location.end_line, Some(5));
}

#[test]
fn an_empty_file_is_its_module_and_implicit_class() {
    let file = parse("empty.gd", "");
    let names: Vec<_> = file
        .symbols
        .iter()
        .map(|s| (s.name.as_str(), s.kind))
        .collect();
    assert_eq!(
        names,
        [
            ("empty.gd", SymbolKind::Module),
            ("empty", SymbolKind::Class)
        ]
    );
    assert!(file.relations.is_empty());
}

#[test]
fn a_bodyless_method_is_abstract_and_its_class_keeps_the_annotation() {
    let file = parse(
        "shape.gd",
        "@abstract\nclass_name Shape\n\n@abstract func area() -> float\n\nfunc label() -> String:\n    return \"shape\"\n",
    );
    symbol(&file, "Shape", SymbolKind::Class);
    let area = detail(&file, "area", SymbolKind::Method);
    assert!(area.modifiers.contains(&Modifier::Abstract), "{area:?}");
    assert_eq!(
        area.return_type.as_ref().map(|t| t.text.as_str()),
        Some("float")
    );
    let label = detail(&file, "label", SymbolKind::Method);
    assert!(!label.modifiers.contains(&Modifier::Abstract), "{label:?}");
    // Both `@abstract` annotations are recorded as references.
    let annotations = common::relations(&file, RelationKind::References, "abstract");
    assert_eq!(annotations.len(), 2, "{annotations:?}");
    assert!(annotations.iter().all(|(_, _, target)| target.external));
}

#[test]
fn static_methods_and_variables_carry_their_modifier() {
    let file = parse(
        "pool.gd",
        "static var created := 0\n\nstatic func make() -> Object:\n    return null\n\nfunc reset():\n    pass\n",
    );
    assert_eq!(
        symbol(&file, "created", SymbolKind::Field)
            .parent
            .as_deref(),
        Some("pool")
    );
    let make = detail(&file, "make", SymbolKind::Method);
    assert!(make.modifiers.contains(&Modifier::Static), "{make:?}");
    assert!(!detail(&file, "reset", SymbolKind::Method)
        .modifiers
        .contains(&Modifier::Static));
}

#[test]
fn init_is_a_method_with_its_parameters() {
    let file = parse(
        "unit.gd",
        "var hp: int\n\nfunc _init(start: int = 3, ...extra: Array) -> void:\n    hp = start\n",
    );
    let init = detail(&file, "_init", SymbolKind::Method);
    assert!(init.parameters_known);
    let params: Vec<_> = init
        .parameters
        .iter()
        .map(|p| {
            (
                p.name.as_deref(),
                p.type_text.as_ref().map(|t| t.text.as_str()),
                p.default.as_deref(),
                p.kind,
            )
        })
        .collect();
    assert_eq!(
        params,
        [
            (
                Some("start"),
                Some("int"),
                Some("3"),
                ParameterKind::Positional
            ),
            (Some("extra"), Some("Array"), None, ParameterKind::Variadic),
        ]
    );
    for name in ["start", "extra"] {
        assert_eq!(
            symbol(&file, name, SymbolKind::Variable).parent.as_deref(),
            Some("_init")
        );
    }
}

#[test]
fn inline_accessors_belong_to_their_property() {
    let file = parse(
        "health.gd",
        "var hp: int = 3:\n    get:\n        return hp\n    set(value):\n        hp = clampi(value, 0, 10)\n",
    );
    // The accessors are not symbols of their own.
    assert_eq!(
        file.symbols
            .iter()
            .filter(|s| matches!(s.kind, SymbolKind::Method | SymbolKind::Function))
            .count(),
        0,
        "{:#?}",
        file.symbols
    );
    assert_eq!(
        symbol(&file, "value", SymbolKind::Variable)
            .parent
            .as_deref(),
        Some("hp")
    );
    // Uses of the property inside its accessors are owned by the property.
    let uses = common::relations(&file, RelationKind::References, "hp");
    assert!(!uses.is_empty());
    assert!(uses.iter().all(|(owner, _, _)| *owner == "hp"), "{uses:?}");
    // `value` resolves to the setter parameter.
    let (_, target) = relation(&file, "hp", RelationKind::References, "value");
    assert_eq!(target.kind, Some(SymbolKind::Variable));
    assert_eq!(target.qualifier.as_deref(), Some("hp"));
}

#[test]
fn named_accessors_reference_their_methods() {
    let file = parse(
        "mana.gd",
        "var mana: int:\n    get = get_mana, set = set_mana\n\nfunc get_mana() -> int:\n    return 1\n\nfunc set_mana(next: int) -> void:\n    pass\n",
    );
    for accessor in ["get_mana", "set_mana"] {
        let (_, target) = relation(&file, "mana", RelationKind::References, accessor);
        assert_eq!(target.kind, Some(SymbolKind::Method), "{accessor}");
        assert_eq!(target.qualifier.as_deref(), Some("mana"), "{accessor}");
    }
}

#[test]
fn members_signals_and_constants_have_their_kinds() {
    let file = parse(
        "hud.gd",
        "signal changed(amount: int)\n@export var speed := 4\n@onready var label = $Label\nconst LIMIT = 10\nvar plain\n",
    );
    for name in ["changed", "speed", "label", "plain"] {
        assert_eq!(
            symbol(&file, name, SymbolKind::Field).parent.as_deref(),
            Some("hud"),
            "{name}"
        );
    }
    assert_eq!(
        symbol(&file, "LIMIT", SymbolKind::Constant)
            .parent
            .as_deref(),
        Some("hud")
    );
    // A signal's parameters are its own, like a callable's.
    let changed = detail(&file, "changed", SymbolKind::Field);
    assert_eq!(changed.parameters.len(), 1);
    assert_eq!(
        symbol(&file, "amount", SymbolKind::Variable)
            .parent
            .as_deref(),
        Some("changed")
    );
}

#[test]
fn named_enums_own_their_entries_and_unnamed_ones_are_class_constants() {
    let file = parse(
        "levels.gd",
        "enum State { IDLE, RUNNING = 3 }\nenum { LOW, HIGH }\n\nfunc pick():\n    return HIGH\n",
    );
    assert_eq!(
        symbol(&file, "State", SymbolKind::Enum).parent.as_deref(),
        Some("levels")
    );
    // An unnamed enum has no name to reference, so it is not a symbol.
    assert!(!file.symbols.iter().any(|s| s.name.starts_with('<')));
    for (name, owner) in [
        ("IDLE", "State"),
        ("RUNNING", "State"),
        ("LOW", "levels"),
        ("HIGH", "levels"),
    ] {
        assert_eq!(
            symbol(&file, name, SymbolKind::Constant).parent.as_deref(),
            Some(owner),
            "{name}"
        );
    }
    // Its entries are visible from the class by bare name.
    let (_, target) = relation(&file, "pick", RelationKind::References, "HIGH");
    assert_eq!(target.kind, Some(SymbolKind::Constant));
    assert_eq!(target.qualifier.as_deref(), Some("levels"));
    assert_eq!(target.path.as_deref(), Some("levels.gd"));
}

#[test]
fn inner_classes_keep_their_class_parent() {
    let file = parse(
        "world.gd",
        "class_name World\n\nclass Tile:\n    var height := 0\n    class Marker:\n        func show():\n            pass\n",
    );
    assert_eq!(
        symbol(&file, "Tile", SymbolKind::Class).parent.as_deref(),
        Some("World")
    );
    assert_eq!(
        symbol(&file, "height", SymbolKind::Field).parent.as_deref(),
        Some("Tile")
    );
    assert_eq!(
        symbol(&file, "Marker", SymbolKind::Class).parent.as_deref(),
        Some("Tile")
    );
    assert_eq!(
        symbol(&file, "show", SymbolKind::Method).parent.as_deref(),
        Some("Marker")
    );
}

#[test]
fn locals_loop_variables_and_match_bindings_are_variables_of_their_callable() {
    let file = parse(
        "loot.gd",
        "func roll(items: Array, pick):\n    var total := 0\n    const BONUS = 2\n    for item: int in items:\n        total += item\n    match pick:\n        [var first, ..]:\n            return first\n        {\"gold\": var gold}:\n            return gold\n    return total + BONUS\n",
    );
    for name in ["total", "item", "first", "gold"] {
        assert_eq!(
            symbol(&file, name, SymbolKind::Variable).parent.as_deref(),
            Some("roll"),
            "{name}"
        );
    }
    assert_eq!(
        symbol(&file, "BONUS", SymbolKind::Constant)
            .parent
            .as_deref(),
        Some("roll")
    );
    for name in ["total", "item", "first", "gold"] {
        let uses = common::relations(&file, RelationKind::References, name);
        assert!(!uses.is_empty(), "{name}");
        for (owner, _, target) in uses {
            assert_eq!(owner, "roll", "{name}");
            assert_eq!(target.kind, Some(SymbolKind::Variable), "{name}");
            assert_eq!(target.qualifier.as_deref(), Some("roll"), "{name}");
        }
    }
}

#[test]
fn a_local_is_not_visible_before_its_declaration_or_outside_its_block() {
    let file = parse(
        "scope.gd",
        "var later := 1\n\nfunc f(flag):\n    print(later)\n    var later := 2\n    if flag:\n        var inner := 3\n    return inner\n",
    );
    let uses = common::relations(&file, RelationKind::References, "later");
    // The first use is before the local: it binds the field.
    let (_, _, first) = uses
        .iter()
        .find(|(_, relation, _)| relation.location.line == 4)
        .expect("use on line 4");
    assert_eq!(first.kind, Some(SymbolKind::Field));
    // `inner` is out of scope after its block, so it is not bound locally.
    let (_, target) = relation(&file, "f", RelationKind::References, "inner");
    assert_ne!(target.kind, Some(SymbolKind::Variable), "{target:?}");
}

#[test]
fn lambdas_assigned_to_a_name_are_functions() {
    let file = parse(
        "hooks.gd",
        "var on_hit = func(amount): return amount\n\nfunc _ready():\n    var double = func(x): return x * 2\n    var later: Callable\n    later = func(): return 1\n    double.call(on_hit.call(1))\n",
    );
    assert_eq!(
        symbol(&file, "on_hit", SymbolKind::Method)
            .parent
            .as_deref(),
        Some("hooks")
    );
    for name in ["double", "later"] {
        assert_eq!(
            symbol(&file, name, SymbolKind::Function).parent.as_deref(),
            Some("_ready"),
            "{name}"
        );
    }
    assert_eq!(
        symbol(&file, "amount", SymbolKind::Variable)
            .parent
            .as_deref(),
        Some("on_hit")
    );
}

#[test]
fn code_inside_strings_and_comments_is_not_declared() {
    let file = parse(
        "texto.gd",
        "# func not_a_function(): é\nvar label := \"func hidden(): pass — café\"\n\nfunc shown() -> void:\n    pass\n",
    );
    symbol(&file, "label", SymbolKind::Field);
    symbol(&file, "shown", SymbolKind::Method);
    assert!(!has_symbol(&file, "not_a_function", SymbolKind::Method));
    assert!(!has_symbol(&file, "hidden", SymbolKind::Method));
}

/// Documented limit of tree-sitter-gdscript 6.1: Godot 4 accepts Unicode
/// identifiers, but the grammar only accepts ASCII ones, so such a file is
/// rejected as a whole. Non-ASCII text in strings and comments is fine.
#[test]
fn a_non_ascii_identifier_is_a_syntax_error_in_the_grammar() {
    for source in [
        "var café := 1\n",
        "func año():\n    pass\n",
        "var 名前 := 1\n",
    ] {
        let result = mct_core::LanguageParser::parse(
            &mct_lang_gdscript::GdscriptParser,
            &mct_core::SourceFile {
                relative_path: "unicode.gd".into(),
                contents: source.into(),
            },
        );
        assert!(
            matches!(result, Err(mct_core::ParseError::Syntax { line: 1, .. })),
            "{source:?}: {result:?}"
        );
    }
}

#[test]
fn a_syntax_error_reports_its_line() {
    let result = mct_core::LanguageParser::parse(
        &mct_lang_gdscript::GdscriptParser,
        &mct_core::SourceFile {
            relative_path: "broken.gd".into(),
            contents: "func ok():\n    pass\n\nfunc broken(:\n    pass\n".into(),
        },
    );
    match result {
        Err(mct_core::ParseError::Syntax { path, line, .. }) => {
            assert_eq!(path, "broken.gd");
            assert_eq!(line, 4);
        }
        other => panic!("expected a syntax error, got {other:?}"),
    }
}

#[test]
fn deeply_nested_input_does_not_overflow() {
    let depth = 2_000;
    let source = format!("var deep = {}1{}\n", "[".repeat(depth), "]".repeat(depth));
    // Either result is fine; the parser must return.
    let _ = mct_core::LanguageParser::parse(
        &mct_lang_gdscript::GdscriptParser,
        &mct_core::SourceFile {
            relative_path: "deep.gd".into(),
            contents: source,
        },
    );
}
