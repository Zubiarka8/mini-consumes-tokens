//! `search_symbols` with `detail` and `max_tokens`: default compatibility,
//! levels, budget over the whole response, pagination and cache bypass.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use mct_index::{EstimatedCounter, ExcludeSet, Index, TokenCounter};
use mct_mcp_server::server::MctServer;
use mct_mcp_server::toon::decode_table;
use serde_json::{json, Value};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/context-pack-app")
}

async fn server() -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&fixture(), ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

async fn search(server: &MctServer, args: Value) -> String {
    let result = server
        .call_read_only_tool("search_symbols_like", args.as_object().cloned())
        .await
        .unwrap();
    result
        .content
        .iter()
        .filter_map(|b| b.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

fn est(text: &str) -> usize {
    EstimatedCounter.count(text).value
}

fn body(text: &str) -> &str {
    text.find("\nBudget:").map_or(text, |at| &text[..at])
}

#[tokio::test]
async fn omitting_the_new_arguments_keeps_the_page_unchanged() {
    let server = server().await;
    for format in ["text", "toon"] {
        let plain = search(
            &server,
            json!({ "query": "order", "format": format, "cache": false }),
        )
        .await;
        let nulls = search(
            &server,
            json!({ "query": "order", "format": format, "cache": false, "detail": null, "max_tokens": null }),
        )
        .await;
        assert_eq!(plain, nulls);
        assert!(!plain.contains("Budget:"));
    }
}

#[tokio::test]
async fn levels_trade_detail_for_tokens_and_keep_provenance() {
    let server = server().await;
    let run = |detail: &'static str| {
        let server = &server;
        async move {
            search(
                server,
                json!({ "query": "order", "format": "text", "detail": detail, "limit": 12 }),
            )
            .await
        }
    };
    let (overview, summary, detailed) = (
        run("overview").await,
        run("summary").await,
        run("detailed").await,
    );
    for text in [&overview, &summary, &detailed] {
        assert!(text.contains("src/orders.rs:L"), "{text}");
        assert!(text.contains("place_order"), "{text}");
    }
    assert!(!overview.contains("| "), "{overview}");
    assert!(summary.contains("| pub fn place_order("), "{summary}");
    assert!(!summary.contains("validate_order(order)?;"), "{summary}");
    assert!(detailed.contains("validate_order(order)?;"), "{detailed}");
    assert!(est(&overview) < est(&summary) && est(&summary) < est(&detailed));
    // Nothing was cut and no budget was given: no budget metadata at all.
    assert!(!overview.contains("Budget:"), "{overview}");
}

#[tokio::test]
async fn a_tight_budget_lowers_the_tail_drops_rows_and_names_the_continuation() {
    let server = server().await;
    for format in ["text", "toon"] {
        let full = search(
            &server,
            json!({ "query": "order", "format": format, "detail": "detailed", "limit": 10 }),
        )
        .await;
        let budget = est(&full) / 2;
        let text = search(
            &server,
            json!({ "query": "order", "format": format, "detail": "detailed", "limit": 10, "max_tokens": budget }),
        )
        .await;
        assert!(
            est(&text) <= budget,
            "{format}: {} > {budget}\n{text}",
            est(&text)
        );
        assert!(
            text.contains("Budget: partial")
                && (text.contains("lowered=") || text.contains("next: offset=")),
            "{text}"
        );
        assert!(text.contains(&format!("/{budget}")), "{text}");
        if format == "toon" {
            let block: String = text
                .lines()
                .skip(1)
                .take_while(|l| !l.starts_with("Budget:"))
                .collect::<Vec<_>>()
                .join("\n");
            let table = decode_table(&block).unwrap_or_else(|| panic!("invalid TOON\n{text}"));
            assert_eq!(table.name, "matches");
        }
    }
}

#[tokio::test]
async fn pages_resume_at_the_reported_offset_without_gaps_or_repeats() {
    let server = server().await;
    let all = search(
        &server,
        json!({ "query": "order", "format": "text", "detail": "overview", "limit": 100 }),
    )
    .await;
    let names = |t: &str| -> Vec<String> {
        body(t)
            .lines()
            .filter(|l| l.contains("src/") || l.contains("tests/"))
            .map(str::to_string)
            .collect()
    };
    let expected = names(&all);
    assert!(
        expected.len() > 4,
        "fixture should have several matches: {all}"
    );
    let mut seen = Vec::new();
    let mut offset = 0usize;
    for _ in 0..50 {
        let page = search(
            &server,
            json!({ "query": "order", "format": "text", "detail": "overview", "limit": 100, "offset": offset, "max_tokens": 120 }),
        )
        .await;
        seen.extend(names(&page));
        match page
            .split("next: offset=")
            .nth(1)
            .and_then(|r| r.split(';').next())
            .and_then(|n| n.trim().parse::<usize>().ok())
        {
            Some(next) => {
                assert!(next > offset, "no progress\n{page}");
                offset = next;
            }
            None => break,
        }
    }
    assert_eq!(seen, expected);
}

#[tokio::test]
async fn a_budget_below_the_minimum_returns_a_valid_explained_response() {
    let server = server().await;
    let text = search(
        &server,
        json!({ "query": "order", "format": "text", "max_tokens": 1 }),
    )
    .await;
    assert!(text.contains("none fit `max_tokens`"), "{text}");
    assert!(
        text.contains("est. tokens are needed for one row"),
        "{text}"
    );
    assert!(text.contains("next: offset=0"), "{text}");
}

#[tokio::test]
async fn no_matches_and_invalid_arguments_behave() {
    let server = server().await;
    let none = search(
        &server,
        json!({ "query": "zzzqqq", "detail": "summary", "max_tokens": 50 }),
    )
    .await;
    assert!(none.contains("No symbol matches"), "{none}");
    let err = server
        .call_read_only_tool(
            "search_symbols_like",
            json!({ "query": "order", "max_tokens": 0 })
                .as_object()
                .cloned(),
        )
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn budgeted_calls_bypass_the_query_cache() {
    let server = server().await;
    let args = json!({ "query": "order", "detail": "overview", "max_tokens": 500 });
    let first = search(&server, args.clone()).await;
    let second = search(&server, args).await;
    assert_eq!(first, second);
    assert!(!second.contains("unchanged"), "{second}");
}

// --- stabilization: footer cost, complete vs partial, recoverable evidence ---

#[tokio::test]
async fn a_complete_overview_with_no_budget_costs_exactly_the_default() {
    // Cause of the `locate a function` regression: a footer on a page that
    // lost nothing. It must now be byte-identical to the default page.
    let server = server().await;
    for format in ["text", "toon"] {
        let plain = search(
            &server,
            json!({ "query": "order", "format": format, "cache": false, "limit": 12 }),
        )
        .await;
        let overview = search(
            &server,
            json!({ "query": "order", "format": format, "limit": 12, "detail": "overview" }),
        )
        .await;
        if format == "text" {
            // Text rows carry no snippet at the default either.
            assert_eq!(plain, overview);
        }
        assert!(est(&overview) <= est(&plain) + 1, "{format}");
        assert!(!overview.contains("Budget:"), "{overview}");
    }
}

#[tokio::test]
async fn a_cut_page_says_partial_and_how_to_continue_a_whole_one_says_complete() {
    let server = server().await;
    let whole = search(
        &server,
        json!({ "query": "order", "format": "text", "detail": "overview", "limit": 100, "max_tokens": 5000 }),
    )
    .await;
    assert!(whole.contains("Budget: complete ~"), "{whole}");
    assert!(
        !whole.contains("partial") && !whole.contains("next:"),
        "{whole}"
    );

    let cut = search(
        &server,
        json!({ "query": "order", "format": "text", "detail": "overview", "limit": 100, "max_tokens": 120 }),
    )
    .await;
    assert!(cut.contains("Budget: partial"), "{cut}");
    assert!(cut.contains(" of 12 seen"), "{cut}");
    assert!(cut.contains("next: offset="), "{cut}");
    // The header counts the matches, as the default page does, not the rows
    // that fit.
    assert!(cut.starts_with("12 match(es) for `order`"), "{cut}");
}

#[tokio::test]
async fn following_the_continuation_recovers_evidence_a_tight_page_missed() {
    // The `ADR lookup, budget 300` case in miniature: the wanted hit ranks
    // late, so one tight call misses it, but `next: offset=` reaches it.
    let server = server().await;
    let wanted = "tests/orders.rs:L28-L34";
    let first = search(
        &server,
        json!({ "query": "order", "format": "text", "detail": "overview", "limit": 100, "max_tokens": 120 }),
    )
    .await;
    assert!(
        !first.contains(wanted),
        "fixture: wanted hit must rank late\n{first}"
    );
    let mut found = false;
    let mut offset = 0usize;
    for _ in 0..20 {
        let page = search(
            &server,
            json!({ "query": "order", "format": "text", "detail": "overview", "limit": 100, "offset": offset, "max_tokens": 120 }),
        )
        .await;
        if page.contains(wanted) {
            found = true;
            break;
        }
        match page
            .split("next: offset=")
            .nth(1)
            .and_then(|r| r.split(['\n', ';']).next())
            .and_then(|n| n.trim().parse::<usize>().ok())
        {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert!(found, "the continuation must reach the evidence");
}
