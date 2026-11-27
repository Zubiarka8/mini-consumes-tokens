//! The `showcase` fixture end to end: a small Godot project with an abstract
//! base class, a subclass, a resource class, an implicit preloaded class, an
//! autoload, a scene with editor signal connections, a text resource and a
//! GUT test. Each test reads the index the way an MCP tool does. HTTP join
//! points across languages are in `mct-mcp-server/tests/http_join.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_gdscript::GdscriptParser;

const ENTITY: &str = "entities/entity.gd";
const HERO: &str = "entities/hero.gd";
const WEAPON: &str = "items/weapon.gd";
const INVENTORY: &str = "items/inventory.gd";
const STATE: &str = "systems/game_state.gd";
const SCENE: &str = "scenes/dungeon.tscn";
const DOOR: &str = "scenes/door.gd";

fn index() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/showcase");
    mct_corpus::open_fixture_indexed(&root, Arc::new(GdscriptParser))
}

/// One relation site: `from` in `path` at `line`, to `name`.
struct Site<'a> {
    path: &'a str,
    from: &'a str,
    line: u32,
    name: &'a str,
}

const fn site<'a>(path: &'a str, from: &'a str, line: u32, name: &'a str) -> Site<'a> {
    Site {
        path,
        from,
        line,
        name,
    }
}

/// The one hit at `site` among `hits`.
fn one(hits: Vec<RelationHit>, site: &Site<'_>) -> RelationHit {
    let mut found: Vec<_> = hits
        .into_iter()
        .filter(|h| {
            h.relative_path == site.path && h.from_symbol == site.from && h.line == site.line
        })
        .collect();
    assert_eq!(
        found.len(),
        1,
        "{}:{} {} -> {}: {found:#?}",
        site.path,
        site.line,
        site.from,
        site.name
    );
    found.remove(0)
}

fn call(index: &Index, site: &Site<'_>) -> RelationHit {
    one(index.find_callers(site.name).unwrap(), site)
}

fn reference(index: &Index, site: &Site<'_>) -> RelationHit {
    one(index.find_references(site.name).unwrap(), site)
}

#[track_caller]
fn resolves_to(hit: &RelationHit, file: &str) {
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:#?}");
    assert_eq!(hit.candidates[0].relative_path, file, "{hit:#?}");
}

#[test]
fn calls_resolve_through_types_inheritance_preloads_and_autoloads() {
    let index = index();
    for (site, file) in [
        // A typed field and a static call on a `class_name` global.
        (site(HERO, "take_turn", 35, "strike"), WEAPON),
        (site(HERO, "_init", 25, "create"), WEAPON),
        // `:=` with `Stats.new()` types the field as the inner class.
        (site(HERO, "take_turn", 35, "roll"), HERO),
        // `super.x()` and a bare inherited call reach the base class.
        (site(HERO, "_ready", 29, "_ready"), ENTITY),
        (site(HERO, "take_damage", 50, "take_damage"), ENTITY),
        (site(HERO, "take_damage", 51, "is_alive"), ENTITY),
        // A bare call to an own method.
        (site(HERO, "take_turn", 46, "take_damage"), HERO),
        // `:=` with `Pack.new()`, where `Pack` preloads an implicit class.
        (site(HERO, "_ready", 31, "add"), INVENTORY),
        // An autoload from `project.godot`.
        (site(HERO, "take_turn", 44, "record_hit"), STATE),
        (site(HERO, "take_damage", 52, "end_run"), STATE),
        (site(STATE, "_on_died", 45, "count"), ENTITY),
    ] {
        resolves_to(&call(&index, &site), file);
    }
}

#[test]
fn an_untyped_value_is_not_guessed() {
    let index = index();
    // `entity` in a lambda and `slot` in a loop have no declared type.
    for site in [
        site(STATE, "living", 40, "is_alive"),
        site(INVENTORY, "total_weight", 43, "weight"),
    ] {
        let hit = call(&index, &site);
        assert_ne!(hit.resolution, Resolution::Resolved, "{hit:#?}");
    }
}

#[test]
fn an_editor_connection_calls_the_handler_of_the_node_script() {
    let index = index();
    resolves_to(
        &call(&index, &site(SCENE, "dungeon.tscn", 19, "take_turn")),
        HERO,
    );
    resolves_to(
        &call(&index, &site(SCENE, "dungeon.tscn", 20, "_on_body_entered")),
        DOOR,
    );
    // A node without a script has no handler to call.
    assert!(index.find_callers("no_script_here").unwrap().is_empty());
}

#[test]
fn references_reach_classes_signals_and_scripts_named_by_file() {
    let index = index();
    for (site, file) in [
        (site(HERO, "weapon", 6, "Weapon"), WEAPON),
        (
            site(
                "tests/test_weapon.gd",
                "test_strike_adds_power",
                6,
                "Weapon",
            ),
            WEAPON,
        ),
        // A preload is a use of the implicit class it loads.
        (site(ENTITY, "Bag", 13, "inventory"), INVENTORY),
        (site(HERO, "Pack", 4, "inventory"), INVENTORY),
        // A scene, a resource and an autoload name a script by its file,
        // also when the script declares `class_name`.
        (site(SCENE, "dungeon.tscn", 4, "door"), DOOR),
        (site(SCENE, "dungeon.tscn", 3, "hero"), HERO),
        (site(SCENE, "dungeon.tscn", 6, "torch"), "scenes/torch.gd"),
        (
            site("resources/sword.tres", "sword.tres", 3, "weapon"),
            WEAPON,
        ),
        (site("project.godot", "GameState", 7, "game_state"), STATE),
        // A signal, emitted in code and connected in the editor.
        (site(DOOR, "_on_body_entered", 9, "opened"), DOOR),
        (site(SCENE, "dungeon.tscn", 19, "opened"), DOOR),
    ] {
        resolves_to(&reference(&index, &site), file);
    }
}

#[test]
fn an_extends_base_is_one_relation_to_the_base_class() {
    let index = index();
    let hits = index.find_references("Entity").unwrap();
    let from_hero: Vec<_> = hits
        .iter()
        .filter(|h| h.relative_path == HERO && h.from_symbol == "Hero")
        .collect();
    assert_eq!(from_hero.len(), 1, "{from_hero:#?}");
    assert_eq!(from_hero[0].kind, "extends");
    resolves_to(from_hero[0], ENTITY);
}

#[test]
fn dependency_graphs_follow_scripts_scenes_resources_and_autoloads() {
    let index = index();
    for (path, wanted) in [
        (
            SCENE,
            &[HERO, "resources/sword.tres", DOOR, "scenes/torch.gd"][..],
        ),
        ("resources/sword.tres", &[WEAPON][..]),
        ("project.godot", &[STATE][..]),
        (HERO, &[INVENTORY][..]),
    ] {
        let graph = index.dependency_graph(path, 1, 20).unwrap();
        let mut to: Vec<_> = graph.edges.iter().map(|e| e.to.as_str()).collect();
        to.sort_unstable();
        let mut wanted = wanted.to_vec();
        wanted.sort_unstable();
        assert_eq!(to, wanted, "{path}: {graph:#?}");
        assert!(graph.external.is_empty(), "{path}: {graph:#?}");
    }
}

#[test]
fn dead_code_is_only_what_nothing_uses() {
    let index = index();
    let dead: Vec<_> = mct_index::find_dead_code_candidates(&index, None, None)
        .unwrap()
        .into_iter()
        .map(|s| (s.relative_path, s.name))
        .collect();
    // `Bonus` is declared and never used. `Torch` is used only by the scene
    // that attaches it; every other class is reached by code, a scene, a
    // resource, `project.godot` or a test.
    assert_eq!(dead, [(HERO.to_owned(), "Bonus".to_owned())]);
}
