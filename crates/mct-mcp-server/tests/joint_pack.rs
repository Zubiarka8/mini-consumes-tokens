//! `build_context_pack` over several symbols at once (issue #631): shared
//! related symbols are listed once with roles per origin, under one limit
//! and budget, and a cut pack says how to get the rest back.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/server.rs"]
mod server_fixture;
use server_fixture::build_server_at as build_server;

#[path = "common/tokens.rs"]
mod tokens;
use tokens::approx_tokens;

#[path = "common/all_content.rs"]
mod all_content;
use all_content::content_of;

use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

async fn pack(server: &MctServer, args: Value) -> String {
    let Value::Object(map) = args else {
        panic!("arguments must be an object")
    };
    let result = server
        .call_read_only_tool("build_context_pack", Some(map))
        .await
        .unwrap_or_else(|err| panic!("build_context_pack failed: {}", err.message));
    content_of(&result)
}

const LIB_RS: &str = r#"/// Normalizes a raw reading before it is stored.
pub fn normalize_reading(raw: u32, scale: u32) -> u32 {
    raw * scale
}

/// Clamps a reading into the sensor's documented range.
pub fn clamp_reading(value: u32, low: u32, high: u32) -> u32 {
    value.max(low).min(high)
}

/// Records one reading in the shared audit trail.
pub fn audit_reading(value: u32, source: &str) -> u32 {
    value + source.len() as u32
}

/// Converts a reading to the unit the dashboard shows.
pub fn convert_reading(value: u32, factor: u32) -> u32 {
    value / factor.max(1)
}

/// Rounds a reading to the display precision.
pub fn round_reading(value: u32, step: u32) -> u32 {
    value - value % step.max(1)
}

pub fn alpha(raw: u32) -> u32 {
    let v = normalize_reading(raw, 2);
    round_reading(convert_reading(audit_reading(clamp_reading(v, 1, 9), "a"), 3), 2)
}

pub fn beta(raw: u32) -> u32 {
    let v = normalize_reading(raw, 3);
    round_reading(convert_reading(audit_reading(clamp_reading(v, 2, 8), "b"), 4), 2)
}

pub fn gamma(raw: u32) -> u32 {
    let v = normalize_reading(raw, 4);
    round_reading(convert_reading(audit_reading(clamp_reading(v, 3, 7), "c"), 5), 2)
}

/// Runs every pipeline once, for the nightly report.
pub fn run_all_pipelines(raw: u32) -> u32 {
    alpha(raw) + beta(raw) + gamma(raw)
}
"#;

fn fixture() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("mct-joint-pack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), LIB_RS).unwrap();
    root
}

fn rows_named<'t>(text: &'t str, name: &str) -> Vec<&'t str> {
    text.lines()
        .filter(|line| line.starts_with(&format!("  {name},")))
        .collect()
}

#[tokio::test]
async fn a_joint_pack_lists_shared_symbols_once_and_costs_less() {
    let server = build_server(&fixture()).await;
    let names = ["alpha", "beta", "gamma"];
    let mut singles = 0;
    for name in names {
        singles += approx_tokens(&pack(&server, json!({ "symbol": name })).await);
    }
    let joint = pack(
        &server,
        json!({ "symbols": [{"symbol": "alpha"}, {"symbol": "beta"}, {"symbol": "gamma"}] }),
    )
    .await;

    assert!(
        joint.starts_with("Context pack for `alpha, beta, gamma` (depth 1): 3 definition(s)"),
        "{joint}"
    );
    // One row per shared symbol, its roles tagged by origin.
    for helper in [
        "normalize_reading",
        "clamp_reading",
        "audit_reading",
        "convert_reading",
        "round_reading",
    ] {
        let rows = rows_named(&joint, helper);
        assert_eq!(rows.len(), 1, "{helper} once:\n{joint}");
        assert!(
            rows[0].starts_with(&format!("  {helper},callee@alpha+beta+gamma,1,src/lib.rs,")),
            "{joint}"
        );
    }
    let caller = rows_named(&joint, "run_all_pipelines");
    assert_eq!(caller.len(), 1, "{joint}");
    assert!(
        caller[0].starts_with("  run_all_pipelines,caller@alpha+beta+gamma,1,"),
        "{joint}"
    );

    let joint_tokens = approx_tokens(&joint);
    assert!(
        joint_tokens * 10 <= singles * 7,
        "joint pack {joint_tokens} tokens vs {singles} for three single packs"
    );
}

#[tokio::test]
async fn a_cut_joint_pack_says_how_to_recover_the_rest() {
    let server = build_server(&fixture()).await;
    let joint = pack(
        &server,
        json!({
            "symbols": [{"symbol": "alpha"}, {"symbol": "beta"}, {"symbol": "nowhere"}],
            "limit": 2,
            "format": "text",
        }),
    )
    .await;
    assert!(
        joint.contains("(not found: `nowhere` — try search_symbols for a partial name)"),
        "{joint}"
    );
    assert!(
        joint.contains("(showing 2, 4 omitted — pass a higher `limit` to see the rest)"),
        "{joint}"
    );
    assert!(
        joint.contains(
            "Omitted related rows by symbol: alpha 4, beta 4. Pass `limit`: 6 to list them all, \
             or pack one alone with `symbol`: \"alpha\"."
        ),
        "{joint}"
    );
}

#[tokio::test]
async fn symbol_and_symbols_are_exclusive_and_bounded() {
    let server = build_server(&fixture()).await;
    let call = |args: Value| {
        let server = &server;
        async move {
            let Value::Object(map) = args else { panic!() };
            server
                .call_read_only_tool("build_context_pack", Some(map))
                .await
                .map(|_| ())
                .map_err(|err| err.message.to_string())
        }
    };
    let both = call(json!({ "symbol": "alpha", "symbols": [{"symbol": "beta"}] })).await;
    assert_eq!(both.unwrap_err(), "pass `symbol` or `symbols`, not both");
    let empty = call(json!({ "symbols": [] })).await;
    assert_eq!(
        empty.unwrap_err(),
        "`symbols` must list at least one symbol"
    );
    let many: Vec<Value> = (0..9)
        .map(|i| json!({ "symbol": format!("s{i}") }))
        .collect();
    let too_many = call(json!({ "symbols": many })).await;
    assert!(
        too_many.unwrap_err().contains("at most 8 fit one pack"),
        "too many"
    );
    // A repeated entry is packed once: still a single-definition pack.
    let repeated = pack(
        &server,
        json!({ "symbols": [{"symbol": "alpha"}, {"symbol": "alpha"}] }),
    )
    .await;
    assert!(
        repeated.starts_with("Context pack for `alpha` (depth 1): 1 definition(s)"),
        "{repeated}"
    );
}
