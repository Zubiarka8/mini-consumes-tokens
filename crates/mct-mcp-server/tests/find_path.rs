//! `find_path` (issue #633): the shortest call path between two symbols,
//! confirmed only when every call resolved; an ambiguous call makes it a
//! possible path, never a confirmed one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/server.rs"]
mod server_fixture;
use server_fixture::build_server_at as build_server;

#[path = "common/all_content.rs"]
mod all_content;
use all_content::content_of;

use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

async fn path(server: &MctServer, args: Value) -> String {
    let Value::Object(map) = args else {
        panic!("arguments must be an object")
    };
    let result = server
        .call_read_only_tool("find_path", Some(map))
        .await
        .unwrap_or_else(|err| panic!("find_path failed: {}", err.message));
    content_of(&result)
}

const LIB_RS: &str = "pub fn entry() {\n    middle();\n}\n\
pub fn middle() {\n    leaf();\n}\n\
pub fn leaf() {}\n\
pub fn island() {}\n\
pub fn starts_ambiguous() {\n    shared();\n}\n\
pub fn ping() {\n    pong();\n}\n\
pub fn pong() {\n    ping();\n}\n\
pub fn long_way() {\n    step_one();\n    finish();\n}\n\
pub fn step_one() {\n    finish();\n}\n\
pub fn finish() {}\n\
pub fn hands_off() {\n    let f = finish;\n    f();\n}\n";

fn fixture() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("mct-find-path-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("web")).unwrap();
    for (file, contents) in [
        ("src/lib.rs", LIB_RS),
        ("src/a.rs", "pub fn shared() {\n    goal();\n}\n"),
        ("src/b.rs", "pub fn shared() {}\n"),
        ("src/c.rs", "pub fn goal() {}\n"),
        ("src/d.rs", "pub fn twin() {\n    crate::c::goal();\n}\n"),
        ("src/e.rs", "pub fn twin() {}\n"),
        (
            "web/App.tsx",
            "import { Button } from './Button';\nexport function App() {\n  return <Button />;\n}\n",
        ),
        (
            "web/Button.tsx",
            "export function Button() {\n  return <button />;\n}\n",
        ),
    ] {
        std::fs::write(root.join(file), contents).unwrap();
    }
    root
}

#[tokio::test]
async fn a_path_of_resolved_calls_is_confirmed_hop_by_hop() {
    let server = build_server(&fixture()).await;
    let toon = path(&server, json!({ "from": "entry", "to": "leaf" })).await;
    assert!(
        toon.starts_with("Confirmed call path from `entry` to `leaf`: 2 hop(s).\n"),
        "{toon}"
    );
    assert!(
        toon.contains(
            "path[2]{hop,from,path,line,to,target,resolution,candidates}:\n  \
             1,entry,src/lib.rs,2,middle,src/lib.rs:4,resolved,1\n  \
             2,middle,src/lib.rs,5,leaf,src/lib.rs:7,resolved,1\n"
        ),
        "{toon}"
    );
    let text = path(
        &server,
        json!({ "from": "entry", "to": "leaf", "format": "text" }),
    )
    .await;
    assert!(
        text.contains("  1. entry (src/lib.rs:2) calls middle at src/lib.rs:4\n"),
        "{text}"
    );
}

#[tokio::test]
async fn a_path_through_an_ambiguous_call_is_only_possible() {
    let server = build_server(&fixture()).await;
    let toon = path(&server, json!({ "from": "starts_ambiguous", "to": "goal" })).await;
    assert!(
        toon.starts_with(
            "No confirmed call path from `starts_ambiguous` to `goal` within 6 hop(s). \
             Possible path: 2 hop(s), 1 through an ambiguous call"
        ),
        "{toon}"
    );
    assert!(
        toon.contains("  1,starts_ambiguous,src/lib.rs,10,shared,src/a.rs:1,ambiguous,2\n"),
        "{toon}"
    );
    assert!(
        toon.contains("  2,shared,src/a.rs,2,goal,src/c.rs:1,resolved,1\n"),
        "{toon}"
    );
}

#[tokio::test]
async fn no_path_says_how_far_it_looked() {
    let server = build_server(&fixture()).await;
    let none = path(&server, json!({ "from": "island", "to": "leaf" })).await;
    assert_eq!(
        none,
        "No call path from `island` to `leaf` within 6 hop(s), confirmed or possible \
         (1 symbol(s) searched).\n"
    );
    // A cycle ends the walk instead of looping.
    let cycle = path(&server, json!({ "from": "ping", "to": "leaf" })).await;
    assert!(cycle.contains("(2 symbol(s) searched)"), "{cycle}");
    let short = path(
        &server,
        json!({ "from": "entry", "to": "leaf", "max_depth": 1 }),
    )
    .await;
    assert!(
        short.starts_with("No call path from `entry` to `leaf` within 1 hop(s)"),
        "{short}"
    );
    let missing = path(&server, json!({ "from": "entry", "to": "nowhere" })).await;
    assert_eq!(
        missing,
        "No symbol named `nowhere` — try search_symbols for a partial name.\n"
    );
    let same = path(&server, json!({ "from": "leaf", "to": "leaf" })).await;
    assert_eq!(same, "`leaf` and `leaf` are the same symbol.\n");
}

#[tokio::test]
async fn the_shorter_route_wins_and_a_homonym_is_picked_by_path() {
    let server = build_server(&fixture()).await;
    let short = path(&server, json!({ "from": "long_way", "to": "finish" })).await;
    assert!(short.contains(": 1 hop(s).\n"), "{short}");
    // Two `twin`s: only the one in src/d.rs reaches `goal`.
    let d = path(
        &server,
        json!({ "from": "twin", "from_path": "src/d.rs", "to": "goal" }),
    )
    .await;
    assert!(
        d.starts_with("Confirmed call path from `twin` to `goal`: 1 hop(s)."),
        "{d}"
    );
    let e = path(
        &server,
        json!({ "from": "twin", "from_path": "src/e.rs", "to": "goal" }),
    )
    .await;
    assert!(e.starts_with("No call path from `twin` to `goal`"), "{e}");
    // A function taken as a value is a reference, not a call.
    let calls = path(&server, json!({ "from": "hands_off", "to": "finish" })).await;
    assert!(calls.starts_with("No call path"), "{calls}");
    let refs = path(
        &server,
        json!({ "from": "hands_off", "to": "finish", "direction": "references" }),
    )
    .await;
    assert!(
        refs.contains("path from `hands_off` to `finish`: 1 hop(s)."),
        "{refs}"
    );
    // A component rendered by another is a reference hop, named as such.
    let render = path(
        &server,
        json!({ "from": "App", "to": "Button", "direction": "references", "format": "text" }),
    )
    .await;
    assert!(
        render.contains("Confirmed path from `App` to `Button`: 1 hop(s)."),
        "{render}"
    );
    assert!(
        render.contains(" renders Button at web/Button.tsx:1"),
        "{render}"
    );
}
