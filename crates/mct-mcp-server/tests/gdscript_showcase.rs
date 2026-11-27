//! The GDScript `showcase` fixture (`mct-lang-gdscript/tests/fixtures/showcase`)
//! through the production registry and the MCP tools: callers and callees,
//! references, impact analysis and context packs with their GUT tests, dead
//! code and the `project.godot` manifest.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use rmcp::handler::server::wrapper::Parameters;
use serde_json::{json, Value};

#[path = "common/content.rs"]
mod content;

fn server() -> MctServer {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../mct-lang-gdscript/tests/fixtures/showcase");
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let registry = mct_languages::build_registry();
    index.reindex(&registry, true).unwrap();
    MctServer::new(index, registry)
}

/// Tool arguments from JSON, as an MCP client sends them.
fn args<T: serde::de::DeserializeOwned>(value: Value) -> Parameters<T> {
    Parameters(serde_json::from_value(value).unwrap())
}

#[track_caller]
fn has_line(text: &str, wanted: &str) {
    assert!(
        text.lines().any(|line| line.contains(wanted)),
        "{wanted}\n---\n{text}"
    );
}

#[tokio::test]
async fn find_callers_and_find_calls_follow_typed_and_autoload_calls() {
    let server = server();
    let callers = content::content_of(
        &server
            .find_callers(args(json!({"function": "strike", "format": "text"})))
            .await
            .unwrap(),
    );
    has_line(
        &callers,
        "entities/hero.gd:35:26 [gdscript] take_turn --calls--> strike -> items/weapon.gd:28 in Weapon",
    );
    let calls = content::content_of(
        &server
            .find_calls(args(json!({"function": "take_turn", "format": "text"})))
            .await
            .unwrap(),
    );
    for wanted in [
        "take_turn --calls--> roll -> L15 in Stats",
        "take_turn --calls--> record_hit -> systems/game_state.gd:29 in game_state",
        "take_turn --calls--> take_damage -> L49 in Hero",
    ] {
        has_line(&calls, wanted);
    }
}

#[tokio::test]
async fn find_references_lists_each_use_of_a_class() {
    let server = server();
    let text = content::content_of(
        &server
            .find_references(args(json!({"symbol": "Weapon", "format": "text"})))
            .await
            .unwrap(),
    );
    has_line(&text, "9 reference(s):");
    for wanted in [
        "entities/hero.gd:6:13 [gdscript] weapon --references--> Weapon -> items/weapon.gd:1",
        "tests/test_weapon.gd:11:25 [gdscript] test_damage_never_drops_below_the_minimum --calls--> Weapon",
    ] {
        has_line(&text, wanted);
    }
}

#[tokio::test]
async fn impact_analysis_and_context_pack_include_the_gut_tests() {
    let server = server();
    let impact = content::content_of(
        &server
            .impact_analysis(args(json!({"symbol": "Weapon", "format": "text"})))
            .await
            .unwrap(),
    );
    has_line(&impact, "2 likely affected test(s)");
    has_line(
        &impact,
        "tests/test_weapon.gd:6:18 [gdscript] test_strike_adds_power",
    );
    has_line(&impact, "2 direct caller(s)");

    let pack = content::content_of(
        &server
            .build_context_pack(args(json!({"symbol": "strike", "format": "text"})))
            .await
            .unwrap(),
    );
    has_line(&pack, "items/weapon.gd:L28-L30 [gdscript] method strike");
    has_line(&pack, "caller take_turn  entities/hero.gd:L34-L46 method");
    // `sword := Weapon.create(..)` has no type the parser can prove, so the
    // test's call is listed as uncertain, still marked as a test.
    has_line(
        &pack,
        "caller,test test_strike_adds_power  tests/test_weapon.gd:L5-L7",
    );
}

#[tokio::test]
async fn find_dead_code_reports_only_the_unused_inner_class() {
    let server = server();
    let text = content::content_of(
        &server
            .find_dead_code(args(json!({"format": "text"})))
            .await
            .unwrap(),
    );
    has_line(&text, "1 dead-code candidate(s)");
    has_line(&text, "class Bonus L18-L20");
}

#[tokio::test]
async fn get_indexing_status_reads_the_project_manifest() {
    let server = server();
    let text = content::content_of(
        &server
            .get_indexing_status(args(json!({"verbose_dependencies": true})))
            .await
            .unwrap(),
    );
    has_line(&text, "gdscript: 11 files");
    has_line(&text, "project.godot (gdscript, 1 dep(s)):");
    has_line(&text, "res://addons/dialogue/plugin.cfg");
}
