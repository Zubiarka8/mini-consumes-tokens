//! `get_dependency_graph` end to end: a small Rust project on disk whose
//! modules `use` each other (`a -> b -> c -> a` is a cycle, `d` hangs off
//! `a`), indexed for real and queried through the tool handler.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Map, Value};

fn tempdir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let id = nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed));
    let dir = std::env::temp_dir().join(format!("mct-depgraph-test-{id}"));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn fixture() -> (PathBuf, MctServer) {
    let root = tempdir();
    fs::create_dir_all(root.join("src")).unwrap();
    let files = [
        ("src/lib.rs", "mod a;\nmod b;\nmod c;\nmod d;\n"),
        (
            "src/a.rs",
            "use crate::b::fb;\nuse crate::d::fd;\nuse std::collections::HashMap;\npub fn fa() { fb(); fd(); }\n",
        ),
        ("src/b.rs", "use crate::c::fc;\npub fn fb() { fc(); }\n"),
        ("src/c.rs", "use crate::a::fa;\npub fn fc() { fa(); }\n"),
        ("src/d.rs", "pub fn fd() {}\n"),
    ];
    for (path, body) in files {
        fs::write(root.join(path), body).unwrap();
    }
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    (root, MctServer::new(index, registry))
}

async fn call(server: &MctServer, args: Value) -> Result<String, String> {
    let object: Option<Map<String, Value>> = args.as_object().cloned();
    match server
        .call_read_only_tool("get_dependency_graph", object)
        .await
    {
        Ok(result) => Ok(result
            .content
            .first()
            .and_then(|block| block.as_text())
            .map(|t| t.text.clone())
            .unwrap_or_default()),
        Err(e) => Err(format!("{e:?}")),
    }
}

#[tokio::test]
async fn follows_imports_and_reports_the_cycle() {
    let (root, server) = fixture();
    let out = call(&server, json!({ "module": "src/a.rs", "depth": 3 }))
        .await
        .unwrap();
    for module in ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"] {
        assert!(out.contains(module), "missing {module}:\n{out}");
    }
    assert!(out.contains("edges["), "TOON tables by default:\n{out}");
    assert!(out.contains("circular dependencies: 1"), "{out}");
    assert!(
        !out.contains("circular dependencies: none"),
        "the a -> b -> c -> a cycle must be reported:\n{out}"
    );
    assert!(out.contains("truncated: false"), "{out}");
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn acyclic_module_says_so_explicitly() {
    let (root, server) = fixture();
    let out = call(&server, json!({ "module": "src/d.rs" }))
        .await
        .unwrap();
    assert!(out.contains("circular dependencies: none"), "{out}");
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn export_adds_the_rendered_graph() {
    let (root, server) = fixture();
    let dot = call(&server, json!({ "module": "src/a.rs", "export": "dot" }))
        .await
        .unwrap();
    assert!(dot.contains("export (dot):"), "{dot}");
    assert!(dot.contains("digraph"), "{dot}");
    let mermaid = call(
        &server,
        json!({ "module": "src/a.rs", "export": "mermaid", "format": "text" }),
    )
    .await
    .unwrap();
    assert!(mermaid.contains("export (mermaid):"), "{mermaid}");
    assert!(mermaid.contains("flowchart"), "{mermaid}");
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn rejects_bad_arguments() {
    let (root, server) = fixture();
    for module in ["../outside.rs", "/etc/passwd", "src/../../x", "  "] {
        assert!(
            call(&server, json!({ "module": module })).await.is_err(),
            "{module} must be rejected"
        );
    }
    assert!(
        call(&server, json!({ "module": "src/a.rs", "export": "svg" }))
            .await
            .is_err()
    );
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn text_format_is_opt_in() {
    let (root, server) = fixture();
    let out = call(&server, json!({ "module": "src/d.rs", "format": "text" }))
        .await
        .unwrap();
    assert!(out.contains("dependency graph of `src/d.rs`"), "{out}");
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn unknown_module_is_reported_not_found() {
    let (root, server) = fixture();
    let out = call(&server, json!({ "module": "src/missing.rs" }))
        .await
        .unwrap();
    assert!(out.contains("module not found / not indexed"), "{out}");
    fs::remove_dir_all(root).ok();
}
