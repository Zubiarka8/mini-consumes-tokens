//! `prompts/list` and `prompts/get` as a real client sees them, over an
//! in-memory duplex, in the style of `tools_list.rs`. Covers the plain server,
//! the server behind `SessionCompat` (as `mct-mcp-server` runs it) and Tool
//! Attention, and checks `tools/list` did not change.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{MctServer, ServerOptions};
use mct_mcp_server::session_compat::SessionCompat;
use mct_mcp_server::{prompts, ttc};
use rmcp::model::{ErrorCode, GetPromptRequestParams};
use rmcp::service::ServiceError;
use rmcp::ServiceExt;

fn build_server(attention: bool) -> MctServer {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compute-app");
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let mut options = ServerOptions::from_env();
    options.tool_attention = attention;
    MctServer::with_options(index, registry, options)
}

fn get(name: &str, arguments: Option<serde_json::Value>) -> GetPromptRequestParams {
    let mut params = GetPromptRequestParams::default();
    params.name = name.to_string();
    params.arguments = arguments.and_then(|v| v.as_object().cloned());
    params
}

/// Runs `check` with a client connected to `server`, wrapped in
/// `SessionCompat` when `compat` is set.
macro_rules! with_client {
    ($server:expr, $compat:expr, |$client:ident| $body:block) => {{
        let (server_io, client_io) = tokio::io::duplex(64 * 1024);
        let server = $server;
        let task = tokio::spawn(async move {
            if $compat {
                let running = SessionCompat::new(server).serve(server_io).await.unwrap();
                running.waiting().await.unwrap();
            } else {
                let running = server.serve(server_io).await.unwrap();
                running.waiting().await.unwrap();
            }
        });
        let $client = ().serve(client_io).await.unwrap();
        let out = $body;
        $client.cancel().await.unwrap();
        task.await.unwrap();
        out
    }};
}

#[tokio::test]
async fn the_server_advertises_prompts_and_lists_every_workflow() {
    for compat in [false, true] {
        let (capability, listed) = with_client!(build_server(false), compat, |client| {
            let capability = client.peer_info().unwrap().capabilities.prompts.is_some();
            (capability, client.list_all_prompts().await.unwrap())
        });
        assert!(capability, "prompts capability missing (compat={compat})");
        let names: Vec<_> = listed.iter().map(|p| p.name.as_str()).collect();
        let expected: Vec<_> = prompts::WORKFLOWS.iter().map(|w| w.name).collect();
        assert_eq!(names, expected, "compat={compat}");
        assert!(names.contains(&"impact") && names.contains(&"dead-code"));
        assert!(names.contains(&"duplicates") && names.contains(&"overview"));
    }
}

#[tokio::test]
async fn a_prompt_names_only_tools_and_parameters_the_server_has() {
    let server = build_server(false);
    let tools = server.tool_catalog();
    let known: HashSet<&str> = tools.iter().map(|t| &*t.name).collect();
    for w in prompts::WORKFLOWS {
        for part in w.steps.split("Call ").skip(1) {
            let tool: String = part
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            assert!(
                known.contains(tool.as_str()),
                "{}: `{tool}` is not a tool",
                w.name
            );
        }
        // The argument is passed to the first tool the prompt calls.
        if let Some(arg) = &w.arg {
            let first: String = w
                .steps
                .split("Call ")
                .nth(1)
                .unwrap()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let tool = tools.iter().find(|t| *t.name == first).unwrap();
            let schema = serde_json::to_value(&*tool.input_schema).unwrap();
            assert!(
                schema["properties"].get(arg.name).is_some(),
                "{}: tool `{first}` has no parameter `{}`",
                w.name,
                arg.name
            );
        }
    }
}

#[tokio::test]
async fn getting_a_prompt_returns_its_steps_and_a_missing_argument_is_invalid_params() {
    for compat in [false, true] {
        let (text, missing, unknown) = with_client!(build_server(false), compat, |client| {
            let got = client
                .get_prompt(get("impact", Some(serde_json::json!({"symbol": "scale"}))))
                .await
                .unwrap();
            let text = serde_json::to_value(&got.messages[0].content).unwrap()["text"]
                .as_str()
                .unwrap()
                .to_string();
            let missing = client.get_prompt(get("impact", None)).await.unwrap_err();
            let unknown = client.get_prompt(get("nope", None)).await.unwrap_err();
            (text, missing, unknown)
        });
        assert!(text.contains("impact_analysis with symbol=scale"), "{text}");
        for error in [missing, unknown] {
            match error {
                ServiceError::McpError(e) => assert_eq!(e.code, ErrorCode::INVALID_PARAMS),
                other => panic!("expected invalid_params, got {other:?} (compat={compat})"),
            }
        }
    }
}

#[tokio::test]
async fn prompts_work_with_tool_attention_on_and_say_how_to_reach_the_tools() {
    let (text, tools) = with_client!(build_server(true), true, |client| {
        let got = client.get_prompt(get("status", None)).await.unwrap();
        let text = serde_json::to_value(&got.messages[0].content).unwrap()["text"]
            .as_str()
            .unwrap()
            .to_string();
        (text, client.list_all_tools().await.unwrap())
    });
    assert!(text.contains("select_tools"), "{text}");
    assert!(text.contains("get_indexing_status"), "{text}");
    // Asking for a prompt does not widen tools/list: still select_tools alone.
    assert_eq!(
        tools.len(),
        1,
        "{:?}",
        tools.iter().map(|t| &*t.name).collect::<Vec<_>>()
    );
    assert_eq!(&*tools[0].name, "select_tools");
}

#[tokio::test]
async fn tools_list_is_unchanged_by_the_prompts() {
    let tools = with_client!(build_server(false), true, |client| {
        client.list_all_tools().await.unwrap()
    });
    assert_eq!(tools.len(), ttc::KNOWN_TOOL_NAMES.len() - 1);
    let names: HashSet<&str> = tools.iter().map(|t| &*t.name).collect();
    for name in ttc::KNOWN_TOOL_NAMES
        .iter()
        .filter(|n| **n != "select_tools")
    {
        assert!(names.contains(name), "tools/list lost `{name}`");
    }
}
