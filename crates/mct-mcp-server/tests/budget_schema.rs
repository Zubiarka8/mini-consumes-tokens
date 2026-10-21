//! MCP compatibility of `detail` / `max_tokens`: the published input schema
//! keeps both optional on every tool that has them, a payload written for the
//! previous schema still works unchanged, and unknown extras are not the
//! server's business to reject.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

const TOOLS: [&str; 5] = [
    "build_context_pack",
    "search_symbols_like",
    "search_symbols",
    "hybrid_search_like",
    "hybrid_search",
];

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/context-pack-app")
}

async fn server() -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&fixture(), ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

async fn call(server: &MctServer, tool: &str, args: Value) -> Result<String, String> {
    match server
        .call_read_only_tool(tool, args.as_object().cloned())
        .await
    {
        Ok(r) => Ok(r
            .content
            .iter()
            .filter_map(|b| b.as_text())
            .map(|t| t.text.clone())
            .collect::<Vec<_>>()
            .join("\n")),
        Err(e) => Err(e.message.to_string()),
    }
}

#[tokio::test]
async fn both_arguments_are_published_and_never_required() {
    let server = server().await;
    let catalog = server.tool_catalog();
    for name in TOOLS {
        let tool = catalog
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("`{name}` missing from the catalog"));
        let props = tool
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .unwrap();
        let required: Vec<&str> = tool
            .input_schema
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        for field in ["detail", "max_tokens"] {
            assert!(props.contains_key(field), "`{name}` lacks `{field}`");
            assert!(!required.contains(&field), "`{name}` requires `{field}`");
        }
        // `max_tokens` is documented as an estimate, never as a model limit.
        let doc = props["max_tokens"]["description"]
            .as_str()
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            doc.to_lowercase().contains("estimated") && doc.contains("not a model limit"),
            "`{name}`: {doc}"
        );
    }
}

#[tokio::test]
async fn payloads_written_for_the_previous_schema_still_work_in_both_formats() {
    let server = server().await;
    let old_payloads = [
        ("build_context_pack", json!({"symbol": "place_order"})),
        (
            "search_symbols_like",
            json!({"query": "order", "limit": 3, "snippet_lines": 2, "cache": false}),
        ),
        (
            "hybrid_search_like",
            json!({"query": "order", "alpha": 0.0, "top_k": 3, "cache": false}),
        ),
    ];
    for (tool, payload) in old_payloads {
        for format in ["toon", "text"] {
            let mut args = payload.clone();
            args["format"] = json!(format);
            let text = call(&server, tool, args).await.unwrap();
            assert!(
                !text.is_empty() && !text.contains("Budget:"),
                "{tool} {format}"
            );
            assert!(
                text.contains("place_order") || text.contains("order"),
                "{tool}"
            );
        }
    }
}

#[tokio::test]
async fn a_client_that_sends_the_new_arguments_gets_a_response_in_the_format_it_asked() {
    let server = server().await;
    for (tool, base) in [
        ("build_context_pack", json!({"symbol": "place_order"})),
        ("search_symbols_like", json!({"query": "order"})),
        (
            "hybrid_search_like",
            json!({"query": "order", "alpha": 0.0}),
        ),
    ] {
        let mut toon = base.clone();
        toon["detail"] = json!("summary");
        toon["max_tokens"] = json!(400);
        let toon_text = call(&server, tool, toon.clone()).await.unwrap();
        assert!(
            toon_text.contains("]{"),
            "{tool}: TOON table expected\n{toon_text}"
        );
        toon["format"] = json!("text");
        let plain = call(&server, tool, toon).await.unwrap();
        assert!(
            !plain.contains("]{"),
            "{tool}: plain text expected\n{plain}"
        );
    }
}

#[tokio::test]
async fn an_unknown_field_is_ignored_and_a_wrong_type_is_an_invalid_argument() {
    let server = server().await;
    let ok = call(
        &server,
        "search_symbols_like",
        json!({"query": "order", "from_a_future_client": true}),
    )
    .await;
    assert!(ok.is_ok(), "{ok:?}");
    for bad in [json!({"max_tokens": "many"}), json!({"detail": 3})] {
        let mut args = json!({"query": "order"});
        args.as_object_mut()
            .unwrap()
            .extend(bad.as_object().unwrap().clone());
        assert!(
            call(&server, "search_symbols_like", args).await.is_err(),
            "{bad}"
        );
    }
}
