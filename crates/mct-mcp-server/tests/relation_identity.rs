//! Issue #97 end to end, through the real Rust parser and the tool layer: a
//! path-scoped relation query never reports a relation to a same-named
//! definition outside the scoped start, and a module path or receiver call
//! never resolves to an unrelated definition that merely shares its name.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

const FILES: [(&str, &str); 4] = [
    (
        "a/src/lib.rs",
        "pub struct A;\n\
         impl A {\n    pub fn run() {}\n}\n\
         pub fn to_a() { A::run(); }\n\
         pub fn to_b() { B::run(); }\n",
    ),
    (
        "b/src/lib.rs",
        "pub struct B;\nimpl B {\n    pub fn run() {}\n}\n",
    ),
    (
        "c/src/util.rs",
        "pub fn random() -> u8 { 4 }\n\
         pub fn from_str() {}\n\
         pub struct Repo;\n\
         impl Repo {\n    pub fn get(&self) {}\n}\n",
    ),
    (
        "c/src/main.rs",
        "fn main() {\n\
         \x20   let _ = rand::random();\n\
         \x20   serde_json::from_str();\n\
         \x20   let v = vec![1];\n\
         \x20   v.get(0);\n\
         }\n",
    ),
];

fn fixture() -> PathBuf {
    let root = std::env::temp_dir().join(format!("mct-relation-identity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, contents) in FILES {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
    root
}

async fn server(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

async fn call(server: &MctServer, tool: &str, args: Value) -> String {
    let Value::Object(args) = args else {
        unreachable!()
    };
    server
        .call_read_only_tool(tool, Some(args))
        .await
        .unwrap_or_else(|err| panic!("`{tool}` failed: {}", err.message))
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn path_scoped_relation_tools_never_report_a_same_named_target_outside_the_start() {
    let root = fixture();
    let server = server(&root).await;

    let callers = call(
        &server,
        "find_callers",
        json!({ "function": "run", "path": "a" }),
    )
    .await;
    let references = call(
        &server,
        "find_references",
        json!({ "symbol": "run", "path": "a" }),
    )
    .await;
    let impact = call(
        &server,
        "impact_analysis",
        json!({ "symbol": "run", "path": "a" }),
    )
    .await;
    let unscoped = call(&server, "find_callers", json!({ "function": "run" })).await;

    for text in [&callers, &references, &impact] {
        assert!(text.contains("to_a "), "{text}");
        assert!(
            !text.contains("to_b"),
            "B::run leaked into the `a` scope:\n{text}"
        );
    }
    assert!(
        callers.contains("to_a --calls--> run -> L3 in A\n"),
        "{callers}"
    );
    assert!(impact.contains("  1 direct caller(s)\n"), "{impact}");
    // Unscoped, both definitions are starts and each call names its own.
    assert!(
        unscoped.contains("to_b --calls--> run -> b/src/lib.rs:3 in B\n"),
        "{unscoped}"
    );

    let calls = call(&server, "find_calls", json!({ "function": "main" })).await;
    let _ = std::fs::remove_dir_all(&root);
    // `rand::`/`serde_json::` are not `c/src/util.rs`, whatever it defines.
    assert!(
        calls.contains("--calls--> random (unresolved)\n"),
        "{calls}"
    );
    assert!(
        calls.contains("--calls--> from_str (unresolved)\n"),
        "{calls}"
    );
    // `v.get()`: the one same-named method is a candidate, never the target.
    assert!(
        calls.contains("--calls--> get (ambiguous among 1: c/src/util.rs:5 in Repo)\n"),
        "{calls}"
    );
}
