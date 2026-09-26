//! Verifies the live tool catalog the MCP server actually registers (not
//! just the TTC parser in isolation): every tool's `rmcp` description comes
//! from `tools.ttc`'s expansion, not the compiled-in fallback literal, and
//! the whole catalog's footprint stays within the token-reduction budget
//! this migration exists for (see issue #14 / CLAUDE.md's TTC section).

// Test code: an unwrap()/expect() here means a broken test precondition —
// see crates/mct-mcp-server/src/ for the production-code no-unwrap policy.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use mct_mcp_server::ttc;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compute-app")
}

async fn build_server() -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&fixture_root(), ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

#[tokio::test]
async fn every_known_tool_is_registered_with_its_ttc_expanded_description() {
    let server = build_server().await;
    let catalog = server.tool_catalog();

    assert_eq!(
        catalog.len(),
        ttc::KNOWN_TOOL_NAMES.len(),
        "registered tool count drifted from ttc::KNOWN_TOOL_NAMES"
    );

    let entries = ttc::parse(ttc::CATALOG_SOURCE).expect("tools.ttc must parse");
    for tool in &catalog {
        let expected = entries
            .get(tool.name.as_ref())
            .unwrap_or_else(|| panic!("no TTC entry for registered tool `{}`", tool.name))
            .expand();
        let actual = tool
            .description
            .as_deref()
            .unwrap_or_else(|| panic!("tool `{}` has no description at all", tool.name));
        assert_eq!(
            actual, expected,
            "tool `{}`'s live description is not the TTC expansion — the compiled-in \
             fallback leaked through (TTC parse/apply must have silently failed)",
            tool.name
        );
    }
}

#[tokio::test]
async fn every_registered_tool_has_a_non_empty_input_schema_object() {
    // Migrating descriptions must not have disturbed parameter schemas —
    // every tool still advertises its (possibly empty-properties, but
    // present) input schema object to the client.
    let server = build_server().await;
    for tool in server.tool_catalog() {
        assert!(
            tool.input_schema.contains_key("type"),
            "tool `{}` lost its input schema `type` field",
            tool.name
        );
    }
}

#[tokio::test]
async fn the_full_catalogs_serialized_footprint_is_well_under_the_pre_ttc_size() {
    let server = build_server().await;
    let catalog = server.tool_catalog();

    let description_bytes: usize = catalog
        .iter()
        .filter_map(|t| t.description.as_deref())
        .map(str::len)
        .sum();

    // The pre-TTC prose descriptions (measured directly from the strings
    // that were in `#[tool(description = "...")]` before this migration)
    // summed to 6428 bytes across 11 tools; the TTC-expanded catalog started
    // at 2024 bytes (~68.5% smaller) and sits at ~2678 bytes across today's
    // 14 tools after `discover_tool_categories`/`get_tool_schema` (issue
    // #15) were added, then ~2913 bytes across 15 tools once
    // `search_symbols` (issue #58) landed, then ~3097 bytes across 16 tools
    // with `hybrid_search` (issue #61). 3300 is a regression guard with
    // headroom for a couple more tools, not a precise assertion — bump it
    // (with a note here) rather than loosen it silently if a future tool
    // needs the room.
    assert!(
        description_bytes < 3_300,
        "tool catalog description bytes grew to {description_bytes}, expected well under 3300 \
         (TTC's whole point is a compact catalog)"
    );
}
