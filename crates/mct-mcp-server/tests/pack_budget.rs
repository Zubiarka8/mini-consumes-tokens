//! `build_context_pack` with `detail` and `max_tokens`: default-compatibility,
//! the three levels, the estimated budget over the whole rendered response,
//! degradation reporting, the minimal response and TOON validity.

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

async fn try_pack(server: &MctServer, args: Value) -> Result<String, String> {
    let args = args.as_object().cloned();
    match server.call_read_only_tool("build_context_pack", args).await {
        Ok(result) => Ok(result
            .content
            .iter()
            .filter_map(|block| block.as_text())
            .map(|t| t.text.clone())
            .collect::<Vec<_>>()
            .join("\n")),
        Err(error) => Err(error.message.to_string()),
    }
}

async fn pack(server: &MctServer, args: Value) -> String {
    try_pack(server, args).await.unwrap()
}

fn est(text: &str) -> usize {
    EstimatedCounter.count(text).value
}

/// Everything before the `Budget:` line the new arguments add.
fn without_budget_line(text: &str) -> String {
    match text.find("\nBudget:") {
        Some(at) => text[..at].to_string(),
        None => text.to_string(),
    }
}

#[tokio::test]
async fn omitting_the_new_arguments_keeps_the_pack_unchanged() {
    let server = server().await;
    for format in ["text", "toon"] {
        let old = pack(
            &server,
            json!({ "symbol": "place_order", "format": format }),
        )
        .await;
        let nulls = pack(
            &server,
            json!({ "symbol": "place_order", "format": format, "detail": null, "max_tokens": null }),
        )
        .await;
        assert_eq!(old, nulls);
        assert!(!old.contains("Budget:"), "{old}");
        assert!(old.contains("40|     validate_order(order)?;"), "{old}");
    }
}

#[tokio::test]
async fn detailed_with_a_roomy_budget_is_the_old_pack_plus_one_line() {
    let server = server().await;
    for format in ["text", "toon"] {
        let old = pack(
            &server,
            json!({ "symbol": "place_order", "format": format }),
        )
        .await;
        let new = pack(
            &server,
            json!({ "symbol": "place_order", "format": format, "max_tokens": 100000 }),
        )
        .await;
        assert_eq!(without_budget_line(&new), old);
        assert!(
            !new.contains("lowered") && !new.contains("dropped"),
            "{new}"
        );
    }
}

#[tokio::test]
async fn levels_are_independent_contracts() {
    let server = server().await;
    let detailed = pack(&server, json!({ "symbol": "place_order" })).await;
    let summary = pack(
        &server,
        json!({ "symbol": "place_order", "detail": "summary" }),
    )
    .await;
    let overview = pack(
        &server,
        json!({ "symbol": "place_order", "detail": "overview" }),
    )
    .await;

    // Every level keeps identity, provenance and the related symbols' locations.
    for text in [&detailed, &summary, &overview] {
        assert!(
            text.contains("src/orders.rs:L35-L49 [rust] function place_order"),
            "{text}"
        );
        assert!(text.contains("validate_order"), "{text}");
        assert!(text.contains("src/orders.rs"), "{text}");
    }
    // Summary: the signature line, not the body; related signatures stay.
    assert!(summary.contains("35| pub fn place_order("), "{summary}");
    assert!(!summary.contains("40|"), "{summary}");
    assert!(summary.contains("fn validate_order"), "{summary}");
    // Overview: locations only, no signatures, no source.
    assert!(
        !overview.contains("35|") && !overview.contains("fn validate_order"),
        "{overview}"
    );
    // Each strictly cheaper than the one above it.
    assert!(est(&overview) < est(&summary), "{overview}\n{summary}");
    assert!(est(&summary) < est(&detailed), "{summary}\n{detailed}");
    // The next-level handle and what exists to fetch are always present.
    assert!(overview.contains("next: summary~"), "{overview}");
    assert!(summary.contains("next: detailed~"), "{summary}");
    assert!(
        overview.contains("avail:") && overview.contains("callee="),
        "{overview}"
    );
    assert!(!detailed.contains("Budget:"), "default detail adds nothing");
}

#[tokio::test]
async fn escalating_from_overview_to_detailed_recovers_the_source() {
    let server = server().await;
    let overview = pack(
        &server,
        json!({ "symbol": "place_order", "detail": "overview" }),
    )
    .await;
    assert!(!overview.contains("40|"));
    // The overview named the file; the agent asks for the full pack of it.
    assert!(overview.contains("src/orders.rs:L35-L49"));
    let detailed = pack(
        &server,
        json!({ "symbol": "place_order", "path": "src/orders.rs", "detail": "detailed" }),
    )
    .await;
    assert!(
        detailed.contains("40|     validate_order(order)?;"),
        "{detailed}"
    );
}

#[tokio::test]
async fn the_estimated_budget_covers_the_whole_response_and_degradation_is_reported() {
    let server = server().await;
    for format in ["text", "toon"] {
        let full = pack(
            &server,
            json!({ "symbol": "place_order", "format": format, "max_tokens": 100000 }),
        )
        .await;
        for budget in [est(&full), 400, 250, 150, 100, 70] {
            let text = pack(
                &server,
                json!({ "symbol": "place_order", "format": format, "max_tokens": budget }),
            )
            .await;
            let spent = est(&text);
            let minimal = text.contains("No row fits");
            assert!(
                spent <= budget || minimal,
                "{format} budget {budget}: {spent} est. tokens\n{text}"
            );
            assert!(text.contains("Budget: "), "{text}");
            assert!(text.contains(&format!("/{budget}")), "{text}");
            if budget < est(&full) {
                assert!(
                    text.contains("Budget: partial")
                        && (text.contains("lowered=") || text.contains("dropped=")),
                    "a shrunk pack must say so\n{text}"
                );
            }
            // Provenance of what remains is never lost.
            if !minimal {
                assert!(text.contains("src/orders.rs:L35-L49"), "{text}");
            }
        }
    }
}

#[tokio::test]
async fn tail_rows_are_lowered_before_the_head() {
    let server = server().await;
    let full = pack(
        &server,
        json!({ "symbol": "place_order", "max_tokens": 100000 }),
    )
    .await;
    // Just under the full cost: only the least relevant rows lose detail.
    let text = pack(
        &server,
        json!({ "symbol": "place_order", "max_tokens": est(&full) - 30 }),
    )
    .await;
    assert!(
        text.contains("40|     validate_order(order)?;"),
        "definition source kept\n{text}"
    );
    assert!(text.contains("lowered"), "{text}");
}

#[tokio::test]
async fn a_budget_below_the_minimal_metadata_still_returns_a_valid_response() {
    let server = server().await;
    for format in ["text", "toon"] {
        let text = pack(
            &server,
            json!({ "symbol": "place_order", "format": format, "max_tokens": 1 }),
        )
        .await;
        assert!(text.starts_with("Context pack for `place_order`"), "{text}");
        assert!(text.contains("No row fits `max_tokens`"), "{text}");
        assert!(
            text.contains("est. tokens are needed for one row"),
            "{text}"
        );
        assert!(text.contains("dropped"), "{text}");
        // Following the advice works: that budget yields at least a definition row.
        let needed: usize = text
            .split("~")
            .last()
            .and_then(|t| t.split(' ').next())
            .and_then(|n| n.parse().ok())
            .unwrap();
        let again = pack(
            &server,
            json!({ "symbol": "place_order", "format": format, "max_tokens": needed }),
        )
        .await;
        assert!(!again.contains("No row fits"), "{again}");
        assert!(again.contains("src/orders.rs:L35-L49"), "{again}");
    }
}

#[tokio::test]
async fn toon_stays_decodable_after_truncation() {
    let server = server().await;
    for budget in [100000usize, 300, 180, 120] {
        let text = pack(
            &server,
            json!({ "symbol": "place_order", "format": "toon", "max_tokens": budget }),
        )
        .await;
        if let Some(at) = text.find("related[") {
            let block: String = text[at..]
                .lines()
                .take_while(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            let table = decode_table(&block).unwrap_or_else(|| panic!("invalid TOON:\n{text}"));
            assert_eq!(table.name, "related");
            assert!(table
                .rows
                .iter()
                .all(|row| row.len() == table.headers.len()));
        }
    }
}

#[tokio::test]
async fn invalid_arguments_are_rejected_clearly() {
    let server = server().await;
    let zero = try_pack(&server, json!({ "symbol": "place_order", "max_tokens": 0 })).await;
    assert!(zero.unwrap_err().contains("max_tokens"));
    let bad = try_pack(
        &server,
        json!({ "symbol": "place_order", "detail": "everything" }),
    )
    .await;
    assert!(bad.unwrap_err().contains("unknown detail level"));
}

#[tokio::test]
async fn results_are_reproducible() {
    let server = server().await;
    let args = json!({ "symbol": "place_order", "detail": "summary", "max_tokens": 200 });
    assert_eq!(pack(&server, args.clone()).await, pack(&server, args).await);
}

// --- stabilization: footer only when informative, cost of going up a level ---

#[tokio::test]
async fn an_unbudgeted_detailed_pack_is_the_plain_pack() {
    let server = server().await;
    for format in ["text", "toon"] {
        let plain = pack(
            &server,
            json!({ "symbol": "place_order", "format": format }),
        )
        .await;
        let detailed = pack(
            &server,
            json!({ "symbol": "place_order", "format": format, "detail": "detailed" }),
        )
        .await;
        assert_eq!(plain, detailed);
    }
}

#[tokio::test]
async fn a_lower_level_prices_the_levels_above_so_the_caller_can_skip_one() {
    let server = server().await;
    let price = |text: &str, level: &str| -> usize {
        text.split(&format!("{level}~"))
            .nth(1)
            .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("no `{level}~` price in\n{text}"))
    };
    let overview = pack(
        &server,
        json!({ "symbol": "place_order", "detail": "overview" }),
    )
    .await;
    let detailed = pack(&server, json!({ "symbol": "place_order" })).await;
    let summary = pack(
        &server,
        json!({ "symbol": "place_order", "detail": "summary" }),
    )
    .await;
    // The quoted prices are the real sizes of the higher levels (the footer is
    // the only difference, and detailed has none).
    assert_eq!(price(&overview, "detailed"), est(&detailed));
    assert_eq!(price(&summary, "detailed"), est(&detailed));
    assert!(price(&overview, "summary") < price(&overview, "detailed"));
    // One call at `detailed` with a budget replaces overview -> detailed:
    // it costs no more than the two calls together.
    let budget = est(&overview) + est(&detailed);
    let single = pack(
        &server,
        json!({ "symbol": "place_order", "detail": "detailed", "max_tokens": budget }),
    )
    .await;
    assert!(est(&single) < est(&overview) + est(&detailed), "{single}");
    assert!(single.contains("40|"), "full source in one call\n{single}");
}

#[tokio::test]
async fn a_cut_pack_names_what_the_full_pack_would_cost() {
    let server = server().await;
    let full = pack(&server, json!({ "symbol": "place_order" })).await;
    let cut = pack(
        &server,
        json!({ "symbol": "place_order", "max_tokens": est(&full) / 2 }),
    )
    .await;
    assert!(cut.contains("Budget: partial"), "{cut}");
    let quoted: usize = cut
        .split("full=")
        .nth(1)
        .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("no full= in\n{cut}"));
    assert_eq!(
        quoted,
        est(&full),
        "full= is the cost of showing everything"
    );
    assert!(cut.contains("avail:"), "{cut}");
}

#[tokio::test]
async fn leftover_budget_buys_leading_source_lines_instead_of_stranding_it() {
    // Cause of `detailed, max_tokens=1200` returning ~490: the level steps
    // jump from the whole source to the signature, so a budget between the
    // two used a fraction of itself. The definition now keeps as many leading
    // lines as still fit.
    let server = server().await;
    let full = pack(
        &server,
        json!({ "symbol": "place_order", "format": "text" }),
    )
    .await;
    let source_lines = |t: &str| {
        t.lines()
            .filter(|l| {
                l.split('|')
                    .next()
                    .is_some_and(|n| n.trim().parse::<u32>().is_ok())
            })
            .count()
    };
    let mut previous = 0;
    for budget in [est(&full) / 2, est(&full) * 3 / 4, est(&full) * 9 / 10] {
        let text = pack(
            &server,
            json!({ "symbol": "place_order", "format": "text", "max_tokens": budget }),
        )
        .await;
        assert!(
            est(&text) <= budget,
            "budget {budget}: {}\n{text}",
            est(&text)
        );
        assert!(text.contains("Budget: partial"), "{text}");
        assert!(
            text.contains("lowered="),
            "still flagged as lowered\n{text}"
        );
        assert!(
            source_lines(&text) >= previous,
            "more budget, no fewer lines"
        );
        previous = source_lines(&text);
    }
    assert!(
        previous > 1,
        "a 90% budget should keep more than the signature"
    );
}
