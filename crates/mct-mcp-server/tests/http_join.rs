//! HTTP join points across languages (issue #561, ADR-038): a `fetch` or
//! `axios` call with a literal path is linked to the axum or Express handler
//! that declares the same endpoint. The link is proven by the literal value,
//! never by a name, and every existing relation keeps its resolution.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use mct_index::{ExcludeSet, Index, RelationHit, Resolution, ResolutionEvidence};

struct Project {
    root: PathBuf,
    index: Index,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-http-join-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".mct-index")).unwrap();
        for (path, contents) in files {
            write(&root, path, contents);
        }
        let root = root.canonicalize().unwrap();
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index
            .reindex(&mct_languages::build_registry(), true)
            .unwrap();
        Self { root, index }
    }

    fn callers(&self, handler: &str) -> Vec<RelationHit> {
        self.index.find_callers(handler).unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

const AXUM: &str = r#"
use axum::{routing::get, Router};
async fn list_orders() {}
async fn show_order() {}
fn app() -> Router {
    Router::new()
        .route("/api/orders", get(list_orders))
        .route("/api/orders/{id}", get(show_order))
}
"#;

fn only(hits: &[RelationHit]) -> &RelationHit {
    assert_eq!(hits.len(), 1, "{hits:?}");
    &hits[0]
}

#[test]
fn fetch_calls_the_axum_handler_that_declares_the_endpoint() {
    let project = Project::new(&[
        ("server/api.rs", AXUM),
        (
            "web/app.js",
            "function loadOrders() { return fetch(\"/api/orders\"); }",
        ),
    ]);
    let callers = project.callers("list_orders");
    let hit = only(&callers);
    assert_eq!(hit.from_symbol, "loadOrders");
    assert_eq!(hit.relative_path, "web/app.js");
    assert_eq!(hit.kind, "calls");
    assert_eq!(hit.resolution, Resolution::Resolved);
    assert_eq!(hit.evidence, ResolutionEvidence::Declared);
    assert!(hit.target_id.is_some());
    assert_eq!(hit.candidates[0].relative_path, "server/api.rs");
}

#[test]
fn fetch_calls_the_express_handler_that_declares_the_endpoint() {
    let project = Project::new(&[
        (
            "server/app.js",
            "const express = require(\"express\");\nconst app = express();\n\
             function listOrders(req, res) {}\napp.get(\"/api/orders\", listOrders);\n",
        ),
        (
            "web/app.ts",
            "export async function load() { await fetch('/api/orders'); }",
        ),
    ]);
    let hit_list = project.callers("listOrders");
    let hit = only(&hit_list);
    assert_eq!(hit.from_symbol, "load");
    assert_eq!(hit.resolution, Resolution::Resolved);
}

#[test]
fn path_parameters_match_across_spellings() {
    let project = Project::new(&[
        ("server/api.rs", AXUM),
        (
            "server/app.js",
            "const express = require(\"express\");\nfunction getUser(req, res) {}\n\
             express().get(\"/api/users/:id\", getUser);\n",
        ),
        (
            "web/app.js",
            "function a(id) { return fetch(`/api/orders/${id}`); }\n\
             function b(id) { return fetch(`/api/users/${id}`); }\n\
             function c() { return fetch('/api/orders/[id]'); }\n",
        ),
    ]);
    let orders = project.callers("show_order");
    let mut from: Vec<_> = orders.iter().map(|h| h.from_symbol.as_str()).collect();
    from.sort_unstable();
    assert_eq!(from, ["a", "c"], "{orders:?}");
    assert_eq!(only(&project.callers("getUser")).from_symbol, "b");
}

#[test]
fn axios_uses_its_method_name_and_fetch_its_options() {
    let project = Project::new(&[
        (
            "server/api.rs",
            "use axum::{routing::{get, post}, Router};\nasync fn list() {}\nasync fn create() {}\n\
             fn app() -> Router { Router::new().route(\"/notes\", get(list).post(create)) }",
        ),
        (
            "web/app.js",
            "function a() { return axios.get('/notes'); }\n\
             function b() { return fetch('/notes', { method: 'POST' }); }\n",
        ),
    ]);
    assert_eq!(only(&project.callers("list")).from_symbol, "a");
    assert_eq!(only(&project.callers("create")).from_symbol, "b");
}

#[test]
fn what_is_not_a_literal_match_gives_no_link() {
    let project = Project::new(&[
        ("server/api.rs", AXUM),
        (
            "web/app.js",
            "function method() { return fetch('/api/orders', { method: 'POST' }); }\n\
             function concatenated(base) { return fetch(base + '/api/orders'); }\n\
             function middle(x) { return fetch(`/api/${x}/orders`); }\n\
             function other_host() { return fetch('https://example.com/api/orders'); }\n\
             function dynamic_method(m) { return fetch('/api/orders', { method: m }); }\n\
             function spread(o) { return fetch('/api/orders', { ...o }); }\n\
             function other_path() { return fetch('/api/invoices'); }\n",
        ),
    ]);
    assert!(project.callers("list_orders").is_empty());
    assert!(project.callers("show_order").is_empty());
}

#[test]
fn an_endpoint_declared_twice_is_ambiguous_and_picks_none() {
    let project = Project::new(&[
        ("server/api.rs", AXUM),
        (
            "fixtures/app.js",
            "const express = require(\"express\");\nfunction fakeOrders(req, res) {}\n\
             express().get(\"/api/orders\", fakeOrders);\n",
        ),
        (
            "web/app.js",
            "function load() { return fetch('/api/orders'); }",
        ),
    ]);
    for handler in ["list_orders", "fakeOrders"] {
        let hits = project.callers(handler);
        let hit = only(&hits);
        assert_eq!(hit.from_symbol, "load");
        assert_eq!(hit.resolution, Resolution::Ambiguous, "{handler}");
        assert_eq!(hit.evidence, ResolutionEvidence::Candidates);
        assert_eq!(hit.target_id, None);
    }
}

#[test]
fn a_handler_imported_from_elsewhere_or_computed_declares_nothing() {
    let project = Project::new(&[
        (
            "server/api.rs",
            "use axum::{routing::get, Router};\nuse crate::h::imported;\nfn local() {}\n\
             fn app() -> Router { Router::new().route(\"/a\", get(imported))\
             .route(path(), get(local)).route(\"/c\", get(|| async {})) }",
        ),
        (
            "web/app.js",
            "function a() { fetch('/a'); fetch('/b'); fetch('/c'); }",
        ),
    ]);
    assert!(project.callers("imported").is_empty());
    assert!(project.callers("local").is_empty());
}

#[test]
fn a_same_named_function_in_another_scope_is_never_picked() {
    let project = Project::new(&[
        (
            "server/api.rs",
            "use axum::{routing::get, Router};\nfn dup() {}\nmod inner { pub fn dup() {} }\n\
             fn app() -> Router { Router::new().route(\"/b\", get(dup)) }",
        ),
        ("web/app.js", "function a() { fetch('/b'); }"),
    ]);
    let hits = project.callers("dup");
    let hit = only(&hits);
    assert_eq!(hit.resolution, Resolution::Ambiguous, "{hit:?}");
    assert_eq!(hit.target_id, None);
}

#[test]
fn the_link_follows_reindexing_and_leaves_other_relations_alone() {
    let mut project = Project::new(&[
        ("server/api.rs", AXUM),
        (
            "web/app.js",
            "function load() { return fetch('/api/orders'); }",
        ),
    ]);
    let registry = mct_languages::build_registry();

    // No change: the pass writes nothing.
    let report = project.index.reindex(&registry, false).unwrap();
    assert!(!report.published, "{report:?}");

    // The handler moves out of the route: the link goes away.
    write(
        &project.root,
        "server/api.rs",
        "use axum::{routing::get, Router};\nasync fn list_orders() {}\nfn app() -> Router { Router::new() }",
    );
    project
        .index
        .reindex_paths(&registry, &[project.root.join("server/api.rs")])
        .unwrap();
    assert!(project.callers("list_orders").is_empty());

    // The route comes back: so does the link.
    write(&project.root, "server/api.rs", AXUM);
    project
        .index
        .reindex_paths(&registry, &[project.root.join("server/api.rs")])
        .unwrap();
    assert_eq!(only(&project.callers("list_orders")).from_symbol, "load");

    // The frontend file is deleted: the link goes with it.
    fs::remove_file(project.root.join("web/app.js")).unwrap();
    project
        .index
        .reindex_paths(&registry, &[project.root.join("web/app.js")])
        .unwrap();
    assert!(project.callers("list_orders").is_empty());
}
