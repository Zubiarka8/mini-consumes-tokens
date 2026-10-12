//! Tool contracts (issue #17): every tool's PRE/POST/EFFECTS from `tools.ttc`
//! reaches the live catalog as MCP annotations and through `get_tool_schema`,
//! and the server enforces them at runtime.

// Test code: an unwrap()/expect() here means a broken test precondition —
// see crates/mct-mcp-server/src/ for the production-code no-unwrap policy.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{MctServer, ServerOptions};
use mct_mcp_server::ttc::{self, Effect};
use serde_json::{json, Map, Value};

/// Tool Attention on, so `select_tools` is in the catalog too.
async fn build_server() -> MctServer {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compute-app");
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let mut options = ServerOptions::from_env();
    options.tool_attention = true;
    MctServer::with_options(index, registry, options)
}

fn text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|t| t.text.as_str())
        .collect()
}

#[tokio::test]
async fn effects_set_the_mcp_annotations_and_only_reindex_and_select_tools_write() {
    let server = build_server().await;
    let entries = ttc::parse(ttc::CATALOG_SOURCE).unwrap();
    for tool in server.tool_catalog() {
        let annotations = tool
            .annotations
            .as_ref()
            .unwrap_or_else(|| panic!("`{}` has no annotations", tool.name));
        let writes = entries[tool.name.as_ref()].effect() == Effect::Write;
        let expected = matches!(tool.name.as_ref(), "reindex" | "select_tools");
        assert_eq!(writes, expected, "`{}`", tool.name);
        assert_eq!(annotations.read_only_hint, Some(!writes), "`{}`", tool.name);
        if writes {
            assert_eq!(annotations.destructive_hint, Some(false), "`{}`", tool.name);
        }
    }
}

#[tokio::test]
async fn get_tool_schema_documents_every_tools_contract() {
    let server = build_server().await;
    for name in ttc::KNOWN_TOOL_NAMES {
        let mut args = Map::new();
        args.insert("name".into(), json!(name));
        let result = server
            .call_read_only_tool("get_tool_schema", Some(args))
            .await
            .unwrap();
        let body = text(&result);
        for field in ["Contract:\nPRE: ", "\nPOST: ", "\nEFFECTS: "] {
            assert!(
                body.contains(field),
                "`{name}` schema lacks {field:?}:\n{body}"
            );
        }
    }
}

#[tokio::test]
async fn batch_refuses_a_tool_whose_contract_writes() {
    let server = build_server().await;
    let err = server
        .call_read_only_tool("reindex", Some(Map::new()))
        .await
        .unwrap_err();
    assert!(err.message.contains("write effects"), "{}", err.message);
}

/// PRE contract: a blank required text argument is rejected as invalid
/// params by every read-only tool, never answered as an empty query.
#[tokio::test]
async fn every_blank_required_text_argument_is_rejected() {
    let server = build_server().await;
    let mut checked = 0;
    for tool in server.tool_catalog() {
        let required: Vec<&str> = tool
            .input_schema
            .get("required")
            .and_then(Value::as_array)
            .map(|keys| keys.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let props = tool
            .input_schema
            .get("properties")
            .and_then(Value::as_object);
        let is_text = |key: &str| {
            props
                .and_then(|p| p.get(key))
                .and_then(|schema| schema.get("type"))
                == Some(&json!("string"))
        };
        if required.is_empty() || !required.iter().all(|key| is_text(key)) {
            continue;
        }
        for blank in required.iter().copied() {
            let args: Map<String, Value> = required
                .iter()
                .map(|key| {
                    let value = if *key == blank { "  " } else { "compute" };
                    ((*key).to_string(), json!(value))
                })
                .collect();
            let err = server
                .call_read_only_tool(&tool.name, Some(args))
                .await
                .expect_err(&format!("`{}` accepted a blank `{blank}`", tool.name));
            assert_eq!(
                err.code,
                rmcp::model::ErrorCode::INVALID_PARAMS,
                "`{}` / `{blank}`: {}",
                tool.name,
                err.message
            );
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} required text args checked");
}
