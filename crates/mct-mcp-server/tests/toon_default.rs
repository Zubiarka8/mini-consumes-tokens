//! `format` defaults to TOON: omitting it, sending null or a blank string, and
//! sending `"toon"` give the same content on every tool that has a `format`
//! argument, `"text"` keeps the labelled text, and `batch` sub-queries and the
//! query cache follow the same rule. The calls go through the real tool
//! handlers (`call_read_only_tool` deserializes the JSON arguments and runs
//! the same method a direct call does), so the JSON `null` case is covered too.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{
    BatchArgs, BatchQuery, FindSymbolArgs, ListSymbolsArgs, MctServer, SearchSymbolsArgs,
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use serde_json::{json, Map, Value};

fn content_of(result: &CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

fn server() -> MctServer {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/many-callers");
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

/// A TOON table header such as `symbols[3]{kind,name,...}:`.
fn is_toon(output: &str) -> bool {
    output.contains("]{")
}

fn object(args: &Value) -> Option<Map<String, Value>> {
    args.as_object().cloned()
}

async fn call(server: &MctServer, tool: &str, args: Value) -> String {
    let result = server
        .call_read_only_tool(tool, object(&args))
        .await
        .unwrap_or_else(|e| panic!("{tool} {args}: {e:?}"));
    content_of(&result)
}

/// Every tool with a `format` argument, with arguments that hit the
/// `many-callers` fixture. `cache: false` keeps the two cached search tools
/// from answering a repeat with the one-line "unchanged" note.
fn calls() -> Vec<(&'static str, Value)> {
    vec![
        ("list_symbols", json!({ "path": "src/lib.rs" })),
        ("find_symbol", json!({ "name": "target" })),
        (
            "search_symbols",
            json!({ "query": "target", "cache": false }),
        ),
        (
            "hybrid_search",
            json!({ "query": "target", "alpha": 0.0, "cache": false }),
        ),
        ("find_references", json!({ "symbol": "target" })),
        ("find_calls", json!({ "function": "caller_001" })),
        ("find_callers", json!({ "function": "target" })),
        ("impact_analysis", json!({ "symbol": "target" })),
        ("find_dead_code", json!({})),
        ("build_context_pack", json!({ "symbol": "target" })),
    ]
}

fn with_format(args: &Value, format: Value) -> Value {
    let mut args = args.clone();
    args["format"] = format;
    args
}

#[tokio::test]
async fn omitted_null_blank_and_explicit_toon_return_the_same_toon() {
    let server = server();
    for (tool, args) in calls() {
        let omitted = call(&server, tool, args.clone()).await;
        let null = call(&server, tool, with_format(&args, Value::Null)).await;
        let blank = call(&server, tool, with_format(&args, json!(" "))).await;
        let toon = call(&server, tool, with_format(&args, json!("toon"))).await;
        assert!(is_toon(&toon), "{tool}: explicit toon is not TOON:\n{toon}");
        assert_eq!(omitted, toon, "{tool}: omitted format differs from toon");
        assert_eq!(null, toon, "{tool}: null format differs from toon");
        assert_eq!(blank, toon, "{tool}: blank format differs from toon");
    }
}

#[tokio::test]
async fn explicit_text_keeps_the_labelled_text_output() {
    let server = server();
    for (tool, args) in calls() {
        let text = call(&server, tool, with_format(&args, json!("text"))).await;
        let toon = call(&server, tool, args.clone()).await;
        assert!(!text.is_empty(), "{tool}: empty text output");
        assert!(
            !is_toon(&text),
            "{tool}: text output looks like TOON:\n{text}"
        );
        assert_ne!(text, toon, "{tool}: text and the default are identical");
    }
}

#[tokio::test]
async fn an_invalid_format_is_still_rejected_on_every_tool() {
    let server = server();
    for (tool, args) in calls() {
        let err = server
            .call_read_only_tool(tool, object(&with_format(&args, json!("xml"))))
            .await
            .unwrap_err();
        assert!(
            err.message.contains("format must be one of text, toon"),
            "{tool}: {err:?}"
        );
    }
}

#[tokio::test]
async fn direct_handler_calls_default_to_toon_and_text_is_opt_in() {
    let server = server();
    let list = |format: Option<&str>| ListSymbolsArgs {
        path: "src/lib.rs".to_string(),
        kind: None,
        language: None,
        limit: None,
        format: format.map(str::to_string),
    };
    let default = content_of(&server.list_symbols(Parameters(list(None))).await.unwrap());
    let toon = content_of(
        &server
            .list_symbols(Parameters(list(Some("toon"))))
            .await
            .unwrap(),
    );
    let text = content_of(
        &server
            .list_symbols(Parameters(list(Some("text"))))
            .await
            .unwrap(),
    );
    assert_eq!(default, toon);
    assert!(is_toon(&default) && !is_toon(&text), "{default}\n{text}");

    let find = |format: Option<&str>| FindSymbolArgs {
        name: "target".to_string(),
        match_mode: None,
        path: None,
        language: None,
        limit: None,
        format: format.map(str::to_string),
    };
    let default = content_of(&server.find_symbol(Parameters(find(None))).await.unwrap());
    assert!(is_toon(&default), "{default}");
}

#[tokio::test]
async fn batch_sub_queries_follow_the_same_default() {
    let server = server();
    let queries: Vec<BatchQuery> = calls()
        .into_iter()
        .map(|(tool, args)| BatchQuery {
            tool: tool.to_string(),
            args: object(&args),
        })
        .collect();
    let batch = content_of(
        &server
            .batch(Parameters(BatchArgs { queries }))
            .await
            .unwrap(),
    );
    for (i, (tool, args)) in calls().into_iter().enumerate() {
        let header = format!("[{}] {tool}\n", i + 1);
        let start = batch.find(&header).unwrap_or_else(|| panic!("{header}")) + header.len();
        let end = batch[start..]
            .find("\n[")
            .map_or(batch.len(), |at| start + at);
        let section = batch[start..end].trim_end();
        let direct = call(&server, tool, args).await;
        assert!(
            is_toon(section),
            "{tool}: batch section is not TOON:\n{section}"
        );
        assert_eq!(
            section,
            direct.trim_end(),
            "{tool}: batch differs from a direct call"
        );
    }

    let text = content_of(
        &server
            .batch(Parameters(BatchArgs {
                queries: vec![BatchQuery {
                    tool: "find_symbol".to_string(),
                    args: object(&json!({ "name": "target", "format": "text" })),
                }],
            }))
            .await
            .unwrap(),
    );
    assert!(
        !is_toon(&text),
        "explicit text in a batch stays text:\n{text}"
    );
}

#[tokio::test]
async fn the_query_cache_never_replays_a_response_in_the_other_format() {
    let server = server();
    let search = |format: Option<&str>| SearchSymbolsArgs {
        query: "target".to_string(),
        path: None,
        language: None,
        limit: None,
        offset: None,
        snippet_lines: None,
        format: format.map(str::to_string),
        cache: None,
        detail: None,
        max_tokens: None,
    };
    let run = |format: Option<&str>| {
        let args = search(format);
        let server = &server;
        async move { content_of(&server.search_symbols(Parameters(args)).await.unwrap()) }
    };

    let first = run(None).await;
    assert!(is_toon(&first), "{first}");
    // The same default call again is a cache hit: the one-line note.
    let again = run(None).await;
    assert!(again.starts_with("cache: unchanged"), "{again}");
    // The same ranking asked for as text is a different rendering: full text,
    // not the note about the TOON reply.
    let text = run(Some("text")).await;
    assert!(!text.starts_with("cache:") && !is_toon(&text), "{text}");
    // Asking for text again is its own cache hit, and explicit toon shares the
    // default's.
    assert!(run(Some("text")).await.starts_with("cache: unchanged"));
    assert!(run(Some("toon")).await.starts_with("cache: unchanged"));
}
