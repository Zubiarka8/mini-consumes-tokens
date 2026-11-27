//! Relations the GDScript parser emits and the target evidence each one
//! carries for the index to resolve.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{parse, relation, relations};
use mct_core::{JoinKind, JoinRole, RelationKind, SymbolKind};

#[test]
fn extends_a_script_path_is_inheritance_and_an_import_of_that_file() {
    let file = parse("actors/player.gd", "extends \"res://actors/base.gd\"\n");
    for kind in [RelationKind::Extends, RelationKind::Imports] {
        let (site, target) = relation(&file, "player", kind, "base.gd");
        assert_eq!(
            target.path.as_deref(),
            Some("res://actors/base.gd"),
            "{kind:?}"
        );
        assert_eq!(target.kind, Some(SymbolKind::Module), "{kind:?}");
        assert_eq!(site.location.line, 1);
    }
}

#[test]
fn a_relative_script_path_resolves_from_the_script_directory() {
    let file = parse(
        "actors/enemies/slime.gd",
        "extends \"../base.gd\"\nconst Loot = preload(\"./loot.gd\")\n",
    );
    let (_, base) = relation(&file, "slime", RelationKind::Extends, "base.gd");
    assert_eq!(base.path.as_deref(), Some("actors/base.gd"));
    let (_, loot) = relation(&file, "Loot", RelationKind::Imports, "loot.gd");
    assert_eq!(loot.path.as_deref(), Some("actors/enemies/loot.gd"));
}

#[test]
fn paths_that_are_not_local_scripts_are_not_imports() {
    let file = parse(
        "a.gd",
        concat!(
            "const Up = preload(\"../../outside.gd\")\n",
            "const Abs = preload(\"/etc/abs.gd\")\n",
            "const Win = preload(\"res://dir\\\\win.gd\")\n",
            "const Scene = preload(\"res://level.tscn\")\n",
            "const Dyn = preload(\"res://\" + \"dyn.gd\")\n",
        ),
    );
    assert!(
        file.relations
            .iter()
            .all(|r| r.kind != RelationKind::Imports),
        "{:#?}",
        file.relations
    );
}

#[test]
fn load_and_preload_import_a_script_from_their_owner() {
    let file = parse(
        "spawner.gd",
        "const Mob = preload(\"res://mob.gd\")\n\nfunc spawn():\n    var boss = load(\"res://boss.gd\")\n    return boss\n",
    );
    let (_, mob) = relation(&file, "Mob", RelationKind::Imports, "mob.gd");
    assert_eq!(mob.path.as_deref(), Some("res://mob.gd"));
    let (_, boss) = relation(&file, "spawn", RelationKind::Imports, "boss.gd");
    assert_eq!(boss.path.as_deref(), Some("res://boss.gd"));
}

#[test]
fn extends_a_named_class_carries_its_qualifier() {
    let file = parse(
        "hero.gd",
        "extends CharacterBody2D\n\nclass Pet extends Outer.Animal:\n    pass\n",
    );
    let (_, engine) = relation(&file, "hero", RelationKind::Extends, "CharacterBody2D");
    assert_eq!(engine.kind, Some(SymbolKind::Class));
    assert_eq!(engine.qualifier, None);
    let (_, nested) = relation(&file, "Pet", RelationKind::Extends, "Animal");
    assert_eq!(nested.kind, Some(SymbolKind::Class));
    assert_eq!(nested.qualifier.as_deref(), Some("Outer"));
}

#[test]
fn calls_in_scope_and_through_self_bind_to_the_script_member() {
    let file = parse(
        "mover.gd",
        "func step():\n    move(1)\n    self.move(2)\n\nfunc move(n):\n    pass\n",
    );
    let calls = relations(&file, RelationKind::Calls, "move");
    assert_eq!(calls.len(), 2, "{calls:?}");
    for (owner, _, target) in calls {
        assert_eq!(owner, "step");
        assert_eq!(target.kind, Some(SymbolKind::Method));
        assert_eq!(target.qualifier.as_deref(), Some("mover"));
        assert_eq!(target.path.as_deref(), Some("mover.gd"));
    }
}

#[test]
fn a_typed_receiver_qualifies_the_call_and_an_unknown_one_is_a_member() {
    let file = parse(
        "net.gd",
        "var http: HTTPRequest\n\nfunc stop(other):\n    http.cancel_request()\n    other.cancel_request()\n",
    );
    let calls = relations(&file, RelationKind::Calls, "cancel_request");
    assert_eq!(calls.len(), 2, "{calls:?}");
    let typed = calls
        .iter()
        .find(|(_, r, _)| r.location.line == 4)
        .unwrap()
        .2;
    assert_eq!(typed.qualifier.as_deref(), Some("HTTPRequest"));
    assert!(!typed.member);
    let unknown = calls
        .iter()
        .find(|(_, r, _)| r.location.line == 5)
        .unwrap()
        .2;
    assert!(unknown.member);
    assert_eq!(unknown.qualifier, None);
}

#[test]
fn an_unbound_pascal_case_receiver_is_the_qualifier() {
    let file = parse(
        "hud.gd",
        "func show(panel):\n    ItemCatalog.find(1)\n    panel.find(2)\n    var Local = panel\n    Local.find(3)\n",
    );
    let calls = relations(&file, RelationKind::Calls, "find");
    let at = |line| {
        calls
            .iter()
            .find(|(_, r, _)| r.location.line == line)
            .unwrap()
            .2
    };
    let global = at(2);
    assert_eq!(global.qualifier.as_deref(), Some("ItemCatalog"));
    assert!(!global.member);
    let untyped = at(3);
    assert!(untyped.member);
    assert_eq!(untyped.qualifier, None);
    // A local binding wins over the naming convention.
    let local = at(5);
    assert!(local.member);
    assert_eq!(local.qualifier, None);
}

#[test]
fn new_on_a_class_or_a_preloaded_script_is_a_constructor_call() {
    let file = parse(
        "factory.gd",
        "const Helper = preload(\"res://helper.gd\")\n\nclass Box:\n    pass\n\nfunc make():\n    Helper.new()\n    Box.new()\n    Enemy.new()\n",
    );
    let (_, helper) = relation(&file, "make", RelationKind::Calls, "_init");
    assert_eq!(helper.path.as_deref(), Some("res://helper.gd"));
    assert_eq!(helper.module.as_deref(), Some("helper.gd"));
    assert_eq!(helper.kind, Some(SymbolKind::Method));

    let (_, local) = relation(&file, "make", RelationKind::Calls, "Box");
    assert_eq!(local.kind, Some(SymbolKind::Class));
    assert_eq!(local.qualifier.as_deref(), Some("factory"));
    assert_eq!(local.path.as_deref(), Some("factory.gd"));

    // A global class registered elsewhere with `class_name`.
    let (_, global) = relation(&file, "make", RelationKind::Calls, "Enemy");
    assert_eq!(global.kind, Some(SymbolKind::Class));
    assert_eq!(global.path, None);
}

#[test]
fn super_calls_target_the_base_method() {
    let file = parse(
        "knight.gd",
        "extends \"res://unit.gd\"\n\nfunc _init():\n    super()\n\nfunc _ready():\n    super._ready()\n    super.heal(2)\n",
    );
    let (_, ctor) = relation(&file, "_init", RelationKind::Calls, "_init");
    assert_eq!(ctor.kind, Some(SymbolKind::Method));
    assert_eq!(ctor.path.as_deref(), Some("res://unit.gd"));
    assert_eq!(ctor.module.as_deref(), Some("unit.gd"));
    for name in ["_ready", "heal"] {
        let (_, target) = relation(&file, "_ready", RelationKind::Calls, name);
        assert_eq!(target.path.as_deref(), Some("res://unit.gd"), "{name}");
    }

    let named = parse("archer.gd", "extends Unit\n\nfunc _init():\n    super()\n");
    let (_, target) = relation(&named, "_init", RelationKind::Calls, "_init");
    assert_eq!(target.qualifier.as_deref(), Some("Unit"));
    assert_eq!(target.path, None);
}

#[test]
fn annotations_are_external_references() {
    let file = parse(
        "tool.gd",
        "@tool\nextends Node\n\n@export_range(0, 10) var level := 1\n",
    );
    for name in ["tool", "export_range"] {
        let hits = relations(&file, RelationKind::References, name);
        assert_eq!(hits.len(), 1, "{name}: {hits:?}");
        assert!(hits[0].2.external, "{name}");
    }
}

#[test]
fn http_request_with_a_literal_url_uses_the_endpoint() {
    let file = parse(
        "api.gd",
        concat!(
            "var client: HTTPRequest\n",
            "var untyped\n\n",
            "func save(url):\n",
            "    client.request(\"http://localhost:8080/api/save\", [], HTTPClient.METHOD_POST, \"{}\")\n",
            "    client.request(\"http://localhost:8080/api/load\")\n",
            "    untyped.request(\"http://localhost:8080/api/skip\")\n",
            "    client.request(url)\n",
            "    client.request(\"http://localhost:8080/api/odd\", [], custom_method())\n",
        ),
    );
    let mut points: Vec<_> = file
        .join_points
        .iter()
        .map(|p| {
            assert_eq!(p.kind, JoinKind::HttpEndpoint);
            assert_eq!(p.role, JoinRole::Uses);
            assert_eq!(file.symbols[p.symbol as usize].name, "save");
            (p.location.line, p.value.as_str())
        })
        .collect();
    points.sort();
    assert_eq!(points.len(), 2, "{points:?}");
    assert_eq!(points[0].0, 5);
    assert!(points[0].1.starts_with("POST "), "{points:?}");
    assert!(points[0].1.ends_with("/api/save"), "{points:?}");
    assert_eq!(points[1].0, 6);
    assert!(points[1].1.starts_with("GET "), "{points:?}");
}

#[test]
fn string_literals_are_collected_for_exact_search() {
    let file = parse(
        "text.gd",
        "func f():\n    print(\"Player movement started\")\n    print(r\"raw \\d text\")\n    print(\"\"\"long text\"\"\")\n",
    );
    let texts: Vec<_> = file.literals.iter().map(|l| l.text.as_str()).collect();
    for expected in ["Player movement started", "raw \\d text", "long text"] {
        assert!(texts.contains(&expected), "{expected}: {texts:?}");
    }
}
