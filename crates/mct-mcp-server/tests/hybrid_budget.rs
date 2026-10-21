//! `hybrid_search_like` with `detail` and `max_tokens`. Needs no embedding
//! model: the lexical-only path is what runs without the `semantic` feature,
//! and the selection code is the one `search_symbols_like` uses.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use mct_index::{EstimatedCounter, ExcludeSet, Index, TokenCounter};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/context-pack-app")
}

async fn server_at(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

async fn try_call(server: &MctServer, tool: &str, args: Value) -> Result<String, String> {
    match server
        .call_read_only_tool(tool, args.as_object().cloned())
        .await
    {
        Ok(result) => Ok(result
            .content
            .iter()
            .filter_map(|b| b.as_text())
            .map(|t| t.text.clone())
            .collect::<Vec<_>>()
            .join("\n")),
        Err(error) => Err(error.message.to_string()),
    }
}

async fn hybrid(server: &MctServer, args: Value) -> String {
    try_call(server, "hybrid_search_like", args).await.unwrap()
}

fn est(text: &str) -> usize {
    EstimatedCounter.count(text).value
}

#[tokio::test]
async fn omitted_and_null_arguments_leave_the_page_unchanged() {
    let server = server_at(&fixture()).await;
    for format in ["text", "toon"] {
        let base = json!({ "query": "order", "alpha": 0.0, "format": format, "cache": false });
        let plain = hybrid(&server, base.clone()).await;
        let mut with_nulls = base.clone();
        with_nulls["detail"] = Value::Null;
        with_nulls["max_tokens"] = Value::Null;
        assert_eq!(plain, hybrid(&server, with_nulls).await);
        assert!(!plain.contains("Budget:"));
    }
}

#[tokio::test]
async fn a_complete_overview_costs_exactly_the_default_page() {
    let server = server_at(&fixture()).await;
    let plain = hybrid(
        &server,
        json!({ "query": "order", "alpha": 0.0, "format": "text", "cache": false, "top_k": 100 }),
    )
    .await;
    let overview = hybrid(
        &server,
        json!({ "query": "order", "alpha": 0.0, "format": "text", "top_k": 100, "detail": "overview" }),
    )
    .await;
    assert_eq!(plain, overview);
}

#[tokio::test]
async fn the_budget_covers_the_note_and_the_page_and_names_the_continuation() {
    let server = server_at(&fixture()).await;
    for format in ["text", "toon"] {
        let budget = 150;
        let text = hybrid(
            &server,
            json!({ "query": "order", "alpha": 0.0, "format": format, "top_k": 50, "detail": "overview", "max_tokens": budget }),
        )
        .await;
        assert!(text.starts_with("hybrid: alpha 0"), "note first\n{text}");
        assert!(est(&text) <= budget, "{format}: {}\n{text}", est(&text));
        assert!(text.contains("Budget: partial"), "{text}");
        assert!(text.contains("next: offset="), "{text}");
    }
}

#[tokio::test]
async fn the_levels_apply_to_hybrid_hits_and_cost_less_when_lower() {
    let server = server_at(&fixture()).await;
    let run = |detail: &'static str| {
        let server = &server;
        async move {
            hybrid(
                server,
                json!({ "query": "order", "alpha": 0.0, "format": "text", "top_k": 12, "detail": detail }),
            )
            .await
        }
    };
    let (overview, summary, detailed) = (
        run("overview").await,
        run("summary").await,
        run("detailed").await,
    );
    assert!(!overview.contains("| "), "{overview}");
    assert!(summary.contains("| pub fn place_order("), "{summary}");
    assert!(detailed.contains("validate_order(order)?;"), "{detailed}");
    assert!(est(&overview) < est(&summary) && est(&summary) < est(&detailed));
}

#[tokio::test]
async fn a_quoted_phrase_keeps_its_literal_section_inside_the_budget() {
    let dir = std::env::temp_dir().join(format!("mct-hybrid-budget-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/db.rs"),
        "pub fn connect(url: &str) -> Result<(), String> {\n    \
         if url.is_empty() {\n        \
         return Err(format!(\"Error de conexión con la BD: {}\", url));\n    \
         }\n    Ok(())\n}\n",
    )
    .unwrap();
    let server = server_at(&dir.canonicalize().unwrap()).await;
    let query = "\"Error de conexion con la BD\"";
    let plain = hybrid(
        &server,
        json!({ "query": query, "format": "text", "cache": false }),
    )
    .await;
    let budgeted = hybrid(
        &server,
        json!({ "query": query, "format": "text", "detail": "overview", "max_tokens": 400 }),
    )
    .await;
    assert!(plain.contains("string literal(s)"), "{plain}");
    // The evidence the phrase found is never dropped, with or without budget.
    assert!(
        budgeted.contains("src/db.rs:3 in function connect"),
        "{budgeted}"
    );
    assert!(est(&budgeted) <= 400, "{budgeted}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn invalid_arguments_and_the_alias_behave() {
    let server = server_at(&fixture()).await;
    let bad = try_call(
        &server,
        "hybrid_search_like",
        json!({ "query": "order", "detail": "everything" }),
    )
    .await;
    assert!(bad.is_err(), "unknown detail is an invalid argument");
    let zero = try_call(
        &server,
        "hybrid_search_like",
        json!({ "query": "order", "max_tokens": 0 }),
    )
    .await;
    assert!(zero.is_err(), "zero budget is an invalid argument");
    let args = json!({ "query": "order", "alpha": 0.0, "format": "text", "detail": "summary", "max_tokens": 200 });
    assert_eq!(
        hybrid(&server, args.clone()).await,
        try_call(&server, "hybrid_search", args).await.unwrap()
    );
}
