//! The web/3D fixture (`tests/fixtures/portfolio-3d-app/`, ADR-003 D6)
//! through the MCP tools. P0b covers asset nodes, the `dependency` status
//! parameter and a source-free context pack for an asset; later phases add
//! their own cases here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{
    BuildContextPackArgs, GetIndexingStatusArgs, ListSymbolsArgs, MctServer,
};
use rmcp::handler::server::wrapper::Parameters;

async fn build_server() -> MctServer {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/portfolio-3d-app");
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

fn content_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn every_asset_is_an_asset_symbol() {
    let server = build_server().await;
    let text = content_of(
        &server
            .list_symbols(Parameters(ListSymbolsArgs {
                path: "public".to_string(),
                kind: Some("asset".to_string()),
                language: None,
                limit: None,
                format: None,
            }))
            .await
            .unwrap(),
    );
    assert_eq!(
        text.lines().skip(1).collect::<Vec<_>>().join("\n"),
        "Assets:\n\
         \x20 studio.hdr  public/hdr/studio.hdr  L1\n\
         \x20 robot.glb  public/models/robot.glb  L1\n\
         \x20 robot.gltf  public/models/robot.gltf  L1\n\
         \x20 wood.png  public/textures/wood.png  L1",
        "{text}"
    );
}

#[tokio::test]
async fn an_asset_definition_is_packed_without_source() {
    let server = build_server().await;
    let text = content_of(
        &server
            .build_context_pack(Parameters(BuildContextPackArgs {
                symbol: "robot.gltf".to_string(),
                path: None,
                language: None,
                depth: None,
                limit: None,
                source_lines: None,
                format: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("public/models/robot.gltf"), "{text}");
    assert!(!text.contains("\"asset\""), "glTF JSON leaked: {text}");
}

#[tokio::test]
async fn get_indexing_status_lists_the_manifests_declaring_a_dependency() {
    let server = build_server().await;
    let status = |dependency: Option<&str>| {
        server.get_indexing_status(Parameters(GetIndexingStatusArgs {
            verbose_dependencies: false,
            dependency: dependency.map(str::to_string),
        }))
    };
    let plain = content_of(&status(None).await.unwrap());
    assert!(plain.contains("  asset: 4 files, 4 symbols\n"), "{plain}");
    assert!(!plain.contains("failed to parse"), "{plain}");

    let three = content_of(&status(Some("three")).await.unwrap());
    assert_eq!(three, format!("{plain}Dependency `three` declared by 1 manifest(s):\n  package.json (javascript_typescript): ^0.164.0\n"));
    let dev = content_of(&status(Some("tailwindcss")).await.unwrap());
    assert!(
        dev.ends_with("  package.json (javascript_typescript): ^3.4.3\n"),
        "{dev}"
    );
    let none = content_of(&status(Some("vue")).await.unwrap());
    assert!(
        none.ends_with("Dependency `vue`: not declared by any manifest.\n"),
        "{none}"
    );
}
