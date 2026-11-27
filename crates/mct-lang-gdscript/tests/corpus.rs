//! Long, multi-file fixture corpus (issue #74): `orchard_farm/`, a small
//! Godot 4.4 farming game with an item catalog, an inventory, actors, a
//! day and weather cycle and a market client. The shared checks (size, line
//! ranges, golden snapshot, index round trip, malformed input) come from
//! `mct-corpus`; the tests below pin what GDScript is expected to extract.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{JoinKind, Modifier, RelationKind, SymbolKind};
use mct_index::Resolution;
use mct_lang_gdscript::GdscriptParser;

mct_corpus::standard_tests!(GdscriptParser);

use RelationKind::{Calls, Extends, Imports, References};
use SymbolKind::{Class, Constant, Enum, Field, Function, Method, Module};

const CATALOG: &str = "orchard_farm/items/item_catalog.gd";
const INVENTORY: &str = "orchard_farm/items/inventory.gd";
const ACTOR: &str = "orchard_farm/actors/actor.gd";
const FARMER: &str = "orchard_farm/actors/farmer.gd";
const TOOL_USE: &str = "orchard_farm/actors/tool_use.gd";
const GRID: &str = "orchard_farm/world/farm_grid.gd";
const STATE: &str = "orchard_farm/autoload/game_state.gd";
const MARKET: &str = "orchard_farm/net/market_client.gd";

fn parent(path: &str, name: &str, kind: SymbolKind) -> Option<&'static str> {
    corpus().symbol_parent(path, name, kind)
}

fn parsed(path: &str) -> &'static mct_core::ParsedFile {
    &corpus()
        .files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("no corpus file {path}"))
        .parsed
}

#[test]
fn scripts_are_classes_with_their_members_and_inner_classes() {
    // `class_name` names the script class; without it the file name does.
    assert_eq!(
        parent(CATALOG, "ItemCatalog", Class),
        Some("item_catalog.gd")
    );
    assert_eq!(parent(STATE, "game_state", Class), Some("game_state.gd"));
    assert_eq!(parent(TOOL_USE, "tool_use", Class), Some("tool_use.gd"));
    assert_eq!(parent(CATALOG, "item_catalog.gd", Module), None);
    // Inner classes and their members.
    assert_eq!(parent(CATALOG, "ItemDef", Class), Some("ItemCatalog"));
    assert_eq!(parent(CATALOG, "describe", Method), Some("ItemDef"));
    assert_eq!(parent(INVENTORY, "Slot", Class), Some("Inventory"));
    assert_eq!(parent(INVENTORY, "from_dict", Method), Some("Slot"));
    assert_eq!(parent(GRID, "Tile", Class), Some("FarmGrid"));
    // Signals, exported and onready variables are fields; constants and
    // enum entries are constants.
    assert_eq!(parent(INVENTORY, "slot_changed", Field), Some("Inventory"));
    assert_eq!(parent(ACTOR, "max_health", Field), Some("OrchardActor"));
    assert_eq!(parent(ACTOR, "sprite", Field), Some("OrchardActor"));
    assert_eq!(parent(CATALOG, "MAX_STACK", Constant), Some("ItemCatalog"));
    assert_eq!(parent(CATALOG, "Quality", Enum), Some("ItemCatalog"));
    assert_eq!(parent(CATALOG, "PRIZE", Constant), Some("Quality"));
    // An unnamed enum's entries are constants of the class itself.
    assert_eq!(parent(CATALOG, "SPRING", Constant), Some("ItemCatalog"));
}

#[test]
fn abstract_and_static_methods_carry_their_modifiers() {
    let actor = parsed(ACTOR);
    let wanted = corpus().symbol(ACTOR, "wanted_direction", Method).id;
    let detail = actor
        .symbol_details
        .iter()
        .find(|d| d.symbol == wanted)
        .unwrap();
    assert!(detail.modifiers.contains(&Modifier::Abstract), "{detail:?}");

    let catalog = parsed(CATALOG);
    let find = corpus().symbol(CATALOG, "find", Method).id;
    let detail = catalog
        .symbol_details
        .iter()
        .find(|d| d.symbol == find)
        .unwrap();
    assert!(detail.modifiers.contains(&Modifier::Static), "{detail:?}");
}

#[test]
fn lambdas_used_as_callbacks_stay_inside_their_callable() {
    // An anonymous lambda passed to `sort_custom` is not a named symbol; its
    // calls belong to the enclosing method.
    assert!(corpus().symbols_named("sort_custom").is_empty());
    corpus().relation(INVENTORY, "sort", Calls, "_value_of");
    assert!(corpus()
        .symbols_named("_by_name")
        .iter()
        .all(|(_, symbol)| symbol.kind == Method));
    assert!(!corpus()
        .files
        .iter()
        .flat_map(|file| &file.parsed.symbols)
        .any(|symbol| symbol.kind == Function && symbol.parent.as_deref() == Some("sort")));
}

#[test]
fn script_paths_are_imports_and_path_bases_are_extends() {
    let c = corpus();
    // A path base is the script's class, named after its file.
    c.relation(FARMER, "Farmer", Extends, "actor");
    c.relation(FARMER, "Farmer", Imports, "actor.gd");
    c.relation(FARMER, "InventoryScript", Imports, "inventory.gd");
    c.relation(FARMER, "ToolUse", Imports, "tool_use.gd");
    c.relation(INVENTORY, "Catalog", Imports, "item_catalog.gd");
    c.relation(GRID, "Weather", Imports, "weather.gd");
    // Engine bases are named classes.
    c.relation(ACTOR, "OrchardActor", Extends, "CharacterBody2D");
    c.relation(CATALOG, "ItemCatalog", Extends, "RefCounted");
}

#[test]
fn imports_resolve_to_the_corpus_files() {
    let index = corpus().index();
    let graph = index.dependency_graph(FARMER, 1, 50).unwrap();
    let mut files: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| edge.from == FARMER)
        .map(|edge| edge.to.as_str())
        .collect();
    files.sort_unstable();
    assert_eq!(files, [ACTOR, TOOL_USE, INVENTORY]);
}

#[test]
fn calls_through_global_classes_self_and_super_are_extracted() {
    let c = corpus();
    // A `class_name` global used as a receiver.
    c.relation(INVENTORY, "add", Calls, "has");
    c.relation(FARMER, "_plant", Calls, "find");
    c.relation(STATE, "_ready", Calls, "ensure_loaded");
    // A bare call in scope.
    c.relation(INVENTORY, "add", Calls, "_fill");
    // `super()` and `super.method()` target the base script.
    c.relation(FARMER, "_init", Calls, "_init");
    c.relation(FARMER, "_ready", Calls, "_ready");
    c.relation(FARMER, "to_save_data", Calls, "to_save_data");
    // Constructors through `Class.new()` and a preloaded script.
    c.relation(CATALOG, "_add_seed", Calls, "ItemDef");
    c.relation(FARMER, "_init", Calls, "_init");
    // Annotations are references.
    c.relation(ACTOR, "max_health", References, "export_range");
}

#[test]
fn super_init_resolves_to_the_base_script_through_the_index() {
    let index = corpus().index();
    let inits = index.find_callers("_init").unwrap();
    assert!(
        inits.iter().any(|hit| hit.relative_path == FARMER
            && hit.from_symbol == "_init"
            && hit.resolution == Resolution::Resolved),
        "{inits:?}"
    );
}

/// A call through a `class_name` global (`ItemCatalog.ensure_loaded()`)
/// carries the global as its qualifier, so it binds to that class's method.
#[test]
fn a_global_class_receiver_call_resolves_to_its_method() {
    let index = corpus().index();
    let callers = index.find_callers("ensure_loaded").unwrap();
    let hit = callers
        .iter()
        .find(|hit| hit.from_symbol == "_ready" && hit.relative_path == STATE)
        .expect("GameState._ready calls ItemCatalog.ensure_loaded");
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    assert_eq!(hit.candidates.len(), 1, "{hit:?}");
    assert_eq!(hit.candidates[0].relative_path, CATALOG);
    assert_eq!(hit.candidates[0].parent.as_deref(), Some("ItemCatalog"));
}

#[test]
fn literal_http_requests_are_endpoints_the_market_client_uses() {
    let market = parsed(MARKET);
    let mut endpoints: Vec<_> = market
        .join_points
        .iter()
        .map(|point| {
            assert_eq!(point.kind, JoinKind::HttpEndpoint);
            point.value.as_str()
        })
        .collect();
    endpoints.sort_unstable();
    // `leave_leaderboard` builds its URL at run time, so it is not one.
    assert_eq!(endpoints.len(), 5, "{endpoints:?}");
    for (method, path) in [
        ("GET", "/api/market/prices"),
        ("POST", "/api/market/sell"),
        ("GET", "/api/market/leaderboard"),
        ("HEAD", "/api/market/health"),
        ("PUT", "/api/market/leaderboard"),
    ] {
        assert!(
            endpoints
                .iter()
                .any(|value| value.starts_with(method) && value.ends_with(path)),
            "{method} {path}: {endpoints:?}"
        );
    }
}

#[test]
fn string_literals_are_searchable() {
    let catalog = parsed(CATALOG);
    assert!(catalog.literals.iter().any(|l| l.text == "Pruning shears"));
    let state = parsed(STATE);
    assert!(state
        .literals
        .iter()
        .any(|l| l.text == "Welcome to the orchard."));
}

/// Known limit: the `GameState` autoload is declared in `project.godot`,
/// which the corpus does not have (the corpus parses every file under
/// `project/` as GDScript). Without it, `GameState.notify()` has no class to
/// resolve to; `crates/mct-mcp-server/tests/gdscript.rs` covers autoloads.
#[test]
fn an_autoload_receiver_without_project_godot_is_unresolved() {
    let index = corpus().index();
    let callers = index.find_callers("notify").unwrap();
    assert!(
        callers
            .iter()
            .filter(|hit| hit.relative_path == FARMER)
            .all(|hit| hit.resolution != Resolution::Resolved),
        "{callers:?}"
    );
}
