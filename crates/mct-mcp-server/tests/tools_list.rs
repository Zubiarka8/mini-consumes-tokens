//! Checks the tool catalog as an MCP client receives it over a real
//! `tools/list` round trip, not through `MctServer::tool_catalog()`.
//!
//! Issue #70: `tool_catalog()` read the TTC-patched router while the
//! `#[tool_handler]` expansion served a fresh one built from the `#[tool]`
//! fallback descriptions. Every test that went through `tool_catalog()`
//! passed while clients got something else. This one runs a real rmcp
//! client against the server over an in-memory duplex.

// Test code: an unwrap()/expect() here means a broken test precondition —
// see crates/mct-mcp-server/src/ for the production-code no-unwrap policy.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{MctServer, ServerOptions};
use mct_mcp_server::ttc;
use rmcp::model::{CallToolRequestParams, Tool};
use rmcp::service::{NotificationContext, RoleClient};
use rmcp::ServiceExt;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compute-app")
}

fn build_server() -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&fixture_root(), ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

/// Serves `server` on one end of a duplex, connects a client to the other
/// end, and returns what the client gets back from `tools/list`.
async fn tools_over_the_wire(server: MctServer) -> Vec<Tool> {
    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let server_task = tokio::spawn(async move {
        let running = server.serve(server_io).await.unwrap();
        running.waiting().await.unwrap();
    });
    let client = ().serve(client_io).await.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    client.cancel().await.unwrap();
    server_task.await.unwrap();
    tools
}

#[tokio::test]
async fn tools_list_serves_the_ttc_expanded_descriptions() {
    let tools = tools_over_the_wire(build_server()).await;
    // `select_tools` is only registered with Tool Attention on.
    assert!(tools.iter().all(|t| t.name != "select_tools"));
    assert_eq!(
        tools.len(),
        ttc::KNOWN_TOOL_NAMES.len() - 1,
        "tools/list tool count drifted from ttc::KNOWN_TOOL_NAMES"
    );

    let entries = ttc::parse(ttc::CATALOG_SOURCE).expect("tools.ttc must parse");
    for tool in &tools {
        let expected = entries
            .get(tool.name.as_ref())
            .unwrap_or_else(|| panic!("no TTC entry for served tool `{}`", tool.name))
            .expand();
        let actual = tool
            .description
            .as_deref()
            .unwrap_or_else(|| panic!("tool `{}` served with no description", tool.name));
        assert_eq!(
            actual, expected,
            "tools/list served `{}` with the compiled-in `#[tool]` fallback, not its \
             tools.ttc expansion — is the handler reading a fresh router instead of \
             `self.tool_router`?",
            tool.name
        );
    }
}

#[tokio::test]
async fn tools_list_matches_tool_catalog() {
    // The catalog tests, `batch`'s byte-stability test and mct-eval's
    // "Tool catalog tokens" all measure `tool_catalog()`. They only
    // describe what clients receive if the two are identical.
    let server = build_server();
    let catalog = serde_json::to_value(server.tool_catalog()).unwrap();
    let served = serde_json::to_value(tools_over_the_wire(server).await).unwrap();
    assert_eq!(
        served, catalog,
        "tools/list and MctServer::tool_catalog() disagree"
    );
}

/// Counts `notifications/tools/list_changed`.
struct ListWatcher(Arc<tokio::sync::Notify>);

impl rmcp::ClientHandler for ListWatcher {
    async fn on_tool_list_changed(&self, _: NotificationContext<RoleClient>) {
        self.0.notify_one();
    }
}

/// Issue #11's client sequence over a real transport: connect, list,
/// `select_tools`, notification, list again, call an unlisted tool.
#[tokio::test]
async fn tool_attention_narrows_tools_list_over_the_wire() {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&fixture_root(), ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let mut options = ServerOptions::from_env();
    options.tool_attention = true;
    let server = MctServer::with_options(index, registry, options);

    let (server_io, client_io) = tokio::io::duplex(64 * 1024);
    let server_task = tokio::spawn(async move {
        let running = server.serve(server_io).await.unwrap();
        running.waiting().await.unwrap();
    });
    let changed = Arc::new(tokio::sync::Notify::new());
    let client = ListWatcher(Arc::clone(&changed))
        .serve(client_io)
        .await
        .unwrap();
    let caps = client.peer_info().unwrap().capabilities.tools.clone();
    assert_eq!(caps.and_then(|t| t.list_changed), Some(true));

    let names = |tools: Vec<Tool>| -> Vec<String> {
        tools.into_iter().map(|t| t.name.to_string()).collect()
    };
    assert_eq!(
        names(client.list_all_tools().await.unwrap()),
        ["select_tools"]
    );

    let args = serde_json::json!({"query": "who calls compute, don't reindex"});
    let reply = client
        .call_tool(
            CallToolRequestParams::new("select_tools")
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(reply.is_error, Some(true));
    tokio::time::timeout(std::time::Duration::from_secs(5), changed.notified())
        .await
        .expect("no tools/list_changed after select_tools");

    let listed = names(client.list_all_tools().await.unwrap());
    assert!(listed.contains(&"find_callers".to_string()), "{listed:?}");
    assert!(!listed.contains(&"reindex".to_string()), "{listed:?}");
    assert!(listed.len() < ttc::KNOWN_TOOL_NAMES.len() / 2, "{listed:?}");

    // Not advertised, still callable.
    let args = serde_json::json!({"name": "compute"});
    let found = client
        .call_tool(
            CallToolRequestParams::new("find_symbol")
                .with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(found.is_error, Some(true));

    // `batch` refuses `select_tools` and leaves the advertised set alone.
    let args = serde_json::json!({"queries": [
        {"tool": "select_tools", "args": {"query": "rebuild the index"}}
    ]});
    let batched = client
        .call_tool(
            CallToolRequestParams::new("batch").with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let text = format!("{:?}", batched.content);
    assert!(text.contains("call it directly"), "{text}");
    assert_eq!(names(client.list_all_tools().await.unwrap()), listed);

    client.cancel().await.unwrap();
    server_task.await.unwrap();
}
