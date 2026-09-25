//! In-process tests for progressive tool discovery (issue #15):
//! `discover_tool_categories` (names + one-line descriptions, grouped, no
//! schemas) and `get_tool_schema` (one named tool's full input schema and
//! description). Same in-process pattern as `tests/tools.rs` — calls the
//! generated tool methods directly, bypassing the stdio/JSON-RPC transport.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{GetToolSchemaArgs, MctServer};
use rmcp::handler::server::wrapper::Parameters;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compute-app")
}

fn content_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

async fn build_server() -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&fixture_root(), ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

#[tokio::test]
async fn discover_tool_categories_lists_every_registered_tool_with_no_input_schema() {
    let server = build_server().await;
    let text = content_of(&server.discover_tool_categories().await.unwrap());

    for tool in server.tool_catalog() {
        assert!(
            text.contains(tool.name.as_ref()),
            "discover_tool_categories dropped tool `{}`",
            tool.name
        );
    }
    // No input-schema JSON leaked into the categorized listing — that's the
    // whole point of deferring it to `get_tool_schema`.
    assert!(!text.contains("\"type\": \"object\""));
    assert!(text.contains("get_tool_schema"));
}

#[tokio::test]
async fn get_tool_schema_returns_the_named_tools_description_and_input_schema() {
    let server = build_server().await;
    let text = content_of(
        &server
            .get_tool_schema(Parameters(GetToolSchemaArgs {
                name: "find_symbol".to_string(),
            }))
            .await
            .unwrap(),
    );

    assert!(text.contains("find_symbol"));
    assert!(text.contains("Input schema:"));
    assert!(text.contains("\"name\""), "expected the `name` property to appear in the rendered schema:\n{text}");
}

#[tokio::test]
async fn get_tool_schema_rejects_an_unknown_tool_name() {
    let server = build_server().await;
    let err = server
        .get_tool_schema(Parameters(GetToolSchemaArgs {
            name: "not_a_real_tool".to_string(),
        }))
        .await
        .unwrap_err();
    assert!(err.message.contains("discover_tool_categories"));
}
