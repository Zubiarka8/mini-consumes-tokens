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

#[test]
fn flask_and_fastapi_routes_are_declared_and_clients_use_them() {
    let project = Project::new(&[
        (
            "app/api.py",
            "from flask import Flask\napp = Flask(__name__)\n\
             @app.route('/api/orders/<int:order_id>', methods=['GET', 'DELETE'])\n\
             def order(order_id):\n    pass\n\
             @app.get(\"/api/health\")\ndef health():\n    pass\n\
             @app.post(f\"/api/{x}\")\ndef computed():\n    pass\n",
        ),
        (
            "web/app.js",
            "function a(id) { return fetch(`/api/orders/${id}`); }\n\
             function b(id) { return fetch(`/api/orders/${id}`, { method: 'DELETE' }); }\n\
             function c() { return fetch('/api/orders/1', { method: 'POST' }); }\n",
        ),
        (
            "tests/test_api.py",
            "def test_health(client):\n    client.get('/api/health')\n",
        ),
    ]);
    let mut from: Vec<_> = project
        .callers("order")
        .iter()
        .map(|h| h.from_symbol.clone())
        .collect();
    from.sort_unstable();
    assert_eq!(from, ["a", "b"]);
    assert_eq!(only(&project.callers("health")).from_symbol, "test_health");
    assert!(project.callers("computed").is_empty());
}

#[test]
fn go_routes_are_declared_and_httptest_requests_use_them() {
    let project = Project::new(&[
        (
            "server/main.go",
            "package main\nimport (\"net/http\"; \"github.com/gin-gonic/gin\")\n\
             func listOrders(c *gin.Context) {}\nfunc showOrder(w http.ResponseWriter, r *http.Request) {}\n\
             func setup(r *gin.Engine, mux *http.ServeMux) {\n\
             \tr.GET(\"/api/orders\", listOrders)\n\
             \tmux.HandleFunc(\"GET /api/orders/{id}\", showOrder)\n\
             \tmux.HandleFunc(\"/api/any\", showOrder)\n}\n",
        ),
        (
            "server/main_test.go",
            "package main\nimport (\"net/http\"; \"net/http/httptest\")\n\
             func TestList(t *testing.T) { httptest.NewRequest(\"GET\", \"/api/orders\", nil) }\n\
             func TestShow(t *testing.T) { httptest.NewRequest(http.MethodGet, \"/api/orders/7\", nil) }\n",
        ),
        ("web/app.js", "function a(id) { return fetch(`/api/orders/${id}`); }"),
    ]);
    assert_eq!(only(&project.callers("listOrders")).from_symbol, "TestList");
    let mut from: Vec<_> = project
        .callers("showOrder")
        .iter()
        .map(|h| h.from_symbol.clone())
        .collect();
    from.sort_unstable();
    assert_eq!(from, ["a"]);
}

#[test]
fn spring_routes_use_the_class_prefix_and_mockmvc_uses_them() {
    let project = Project::new(&[
        (
            "src/OrderController.java",
            "import org.springframework.web.bind.annotation.*;\n\
             @RestController\n@RequestMapping(\"/api/orders\")\npublic class OrderController {\n\
             @GetMapping\npublic String list() { return \"\"; }\n\
             @GetMapping(\"/{id}\")\npublic String show() { return \"\"; }\n\
             @RequestMapping(value = \"/{id}\", method = RequestMethod.DELETE)\npublic void remove() {}\n\
             @PostMapping(PATH)\npublic void create() {}\n}\n\
             @RestController\n@RequestMapping(BASE)\nclass Other {\n@GetMapping(\"/x\")\npublic void x() {}\n}\n",
        ),
        (
            "src/OrderTest.java",
            "import org.springframework.test.web.servlet.MockMvc;\nclass OrderTest {\n\
             void listWorks() throws Exception { mvc.perform(get(\"/api/orders\")); }\n\
             void removeWorks() throws Exception { mvc.perform(delete(\"/api/orders/7\")); }\n}\n",
        ),
        (
            "web/app.js",
            "function a(id) { return fetch(`/api/orders/${id}`); }\n\
             function b(id) { return fetch(`/api/orders/${id}`, { method: 'DELETE' }); }",
        ),
    ]);
    assert_eq!(only(&project.callers("list")).from_symbol, "listWorks");
    assert_eq!(only(&project.callers("show")).from_symbol, "a");
    assert_eq!(only(&project.callers("remove")).from_symbol, "b");
    assert!(project.callers("create").is_empty());
    assert!(project.callers("x").is_empty());
}

#[test]
fn kotlin_spring_routes_use_the_class_prefix() {
    let project = Project::new(&[
        (
            "src/Orders.kt",
            "import org.springframework.web.bind.annotation.*\n\
             @RestController\n@RequestMapping(\"/api/orders\")\nclass Orders {\n\
             @GetMapping\nfun listAll(): String = \"\"\n\
             @GetMapping(\"/{id}\")\nfun showOne(): String = \"\"\n\
             @RequestMapping(value = [\"/{id}\"], method = [RequestMethod.DELETE])\nfun dropOne() {}\n\
             @PostMapping(path = \"/x\")\nfun makeOne() {}\n}\n",
        ),
        (
            "web/app.js",
            "function a() { return fetch('/api/orders'); }\n\
             function b(id) { return fetch(`/api/orders/${id}`); }\n\
             function c() { return fetch('/api/orders/x', { method: 'POST' }); }",
        ),
    ]);
    assert_eq!(only(&project.callers("listAll")).from_symbol, "a");
    assert_eq!(only(&project.callers("showOne")).from_symbol, "b");
    assert_eq!(only(&project.callers("makeOne")).from_symbol, "c");
}

#[test]
fn aspnet_controllers_and_minimal_apis_declare_routes() {
    let project = Project::new(&[
        (
            "Api/OrdersController.cs",
            "using Microsoft.AspNetCore.Mvc;\n[ApiController]\n[Route(\"api/orders\")]\n\
             public class OrdersController : ControllerBase {\n\
             [HttpGet]\npublic string ListAll() => \"\";\n\
             [HttpGet(\"{id:int}\")]\npublic string ShowOne(int id) => \"\";\n\
             [HttpDelete(\"~/api/gone\")]\npublic void Gone() {}\n}\n\
             [Route(\"api/[controller]\")]\npublic class TokenController : ControllerBase {\n\
             [HttpGet]\npublic string Tokened() => \"\";\n}\n",
        ),
        (
            "Api/Program.cs",
            "using Microsoft.AspNetCore.Builder;\nvar app = WebApplication.Create();\n\
             app.MapPost(\"/api/ping\", Ping);\n\
             static string Ping() => \"pong\";\n",
        ),
        (
            "Api.Tests/OrdersTests.cs",
            "using System.Net.Http;\nclass OrdersTests {\n\
             async Task ListWorks(HttpClient client) { await client.GetAsync(\"/api/orders\"); }\n\
             async Task PingWorks(HttpClient client) { await client.PostAsync(\"/api/ping\", null); }\n}\n",
        ),
        (
            "web/app.js",
            "function a(id) { return fetch(`/api/orders/${id}`); }\n\
             function b() { return fetch('/api/gone', { method: 'DELETE' }); }\n\
             function c() { return fetch('/api/token'); }",
        ),
    ]);
    let mut list: Vec<_> = project
        .callers("ListAll")
        .iter()
        .map(|h| h.from_symbol.clone())
        .collect();
    list.sort_unstable();
    assert_eq!(list, ["ListWorks"]);
    assert_eq!(only(&project.callers("ShowOne")).from_symbol, "a");
    assert_eq!(only(&project.callers("Gone")).from_symbol, "b");
    assert_eq!(only(&project.callers("Ping")).from_symbol, "PingWorks");
    assert!(project.callers("Tokened").is_empty());
}

#[test]
fn symfony_routes_are_declared_and_laravel_tests_use_them() {
    let project = Project::new(&[
        (
            "src/OrderController.php",
            "<?php\nuse Symfony\\Component\\Routing\\Attribute\\Route;\n\
             #[Route('/api/orders')]\nclass OrderController {\n\
             #[Route('/', methods: ['GET'])]\npublic function index() {}\n\
             #[Route('/{id}', methods: ['GET', 'DELETE'])]\npublic function item() {}\n\
             #[Route('/any')]\npublic function anything() {}\n}\n",
        ),
        (
            "tests/OrderTest.php",
            "<?php\nuse Illuminate\\Foundation\\Testing\\TestCase;\nclass OrderTest extends TestCase {\n\
             public function testIndex() { $this->getJson('/api/orders'); }\n\
             public function testAny() { $this->get('/api/orders/any'); }\n}\n",
        ),
        ("web/app.js", "function a(id) { return fetch(`/api/orders/${id}`); }"),
    ]);
    assert_eq!(only(&project.callers("index")).from_symbol, "testIndex");
    assert_eq!(only(&project.callers("item")).from_symbol, "a");
    assert!(project.callers("anything").is_empty());
}

#[test]
fn curl_to_this_machine_uses_an_endpoint() {
    let project = Project::new(&[
        ("server/api.rs", AXUM),
        (
            "scripts/smoke.sh",
            "#!/usr/bin/env bash\nlist() {\n  curl -s http://localhost:3000/api/orders\n}\n\
             other() {\n  curl -s https://example.com/api/orders\n  curl -s -X POST http://localhost:3000/api/orders\n  curl -s \"http://localhost:3000/$path\"\n}\n",
        ),
    ]);
    let hits = project.callers("list_orders");
    assert_eq!(only(&hits).from_symbol, "list");
}

#[test]
fn powershell_requests_to_this_machine_use_an_endpoint() {
    let project = Project::new(&[
        ("server/api.rs", AXUM),
        (
            "scripts/Smoke.ps1",
            "function Get-Orders {\n  Invoke-RestMethod -Uri \"http://localhost:3000/api/orders\" -Method Get\n}\n\
             function Other {\n  Invoke-RestMethod -Uri http://localhost:3000/api/orders -Method Post\n  \
             Invoke-WebRequest \"https://example.com/api/orders\"\n  Invoke-RestMethod -Uri \"http://localhost:3000/$path\"\n}\n",
        ),
    ]);
    let hits = project.callers("list_orders");
    assert_eq!(only(&hits).from_symbol, "Get-Orders");
}

#[test]
fn lapis_and_httplib_routes_are_declared() {
    let project = Project::new(&[
        (
            "app.lua",
            "local lapis = require(\"lapis\")\nlocal app = lapis.Application()\n\
             local function list_orders(self) return {} end\n\
             app:get(\"/api/orders\", list_orders)\n\
             app:get(\"/api/anon\", function(self) return {} end)\n",
        ),
        (
            "server.cpp",
            "#include <httplib.h>\nvoid show_order(const httplib::Request&, httplib::Response&) {}\n\
             void serve(httplib::Server& svr) { svr.Get(\"/api/orders/:id\", show_order); }\n\
             void probe(httplib::Client& cli) { cli.Get(\"/api/orders\"); }\n",
        ),
        ("web/app.js", "function a(id) { return fetch(`/api/orders/${id}`); }"),
    ]);
    assert_eq!(only(&project.callers("list_orders")).from_symbol, "probe");
    assert_eq!(only(&project.callers("show_order")).from_symbol, "a");
}

#[test]
fn reqwest_requests_to_this_machine_use_an_endpoint() {
    let project = Project::new(&[
        (
            "server/main.rs",
            "use axum::{routing::get, Router};\nasync fn health() {}\n\
             fn app() -> Router { Router::new().route(\"/api/health\", get(health)) }\n",
        ),
        (
            "client/main.rs",
            "async fn ping(client: &reqwest::Client) {\n\
             \x20   client.get(\"http://localhost:3000/api/health\").send().await;\n\
             \x20   let _ = reqwest::get(\"https://example.com/api/health\").await;\n\
             \x20   let _ = reqwest::get(\"/api/health\").await;\n}\n\
             async fn direct() { reqwest::get(\"http://127.0.0.1/api/health\").await; }\n",
        ),
        (
            "other/lookup.rs",
            "fn f(map: &std::collections::HashMap<String, u8>) { map.get(\"/api/health\"); }\n",
        ),
    ]);
    let mut from: Vec<_> = project
        .callers("health")
        .iter()
        .map(|h| h.from_symbol.clone())
        .collect();
    from.sort_unstable();
    assert_eq!(from, ["direct", "ping"]);
}
