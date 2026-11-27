#![allow(clippy::unwrap_used)]

use std::path::Path;

use mct_index::{ExcludeSet, Index, QueryScope, Resolution};
use mct_mcp_server::server::{GetFileSkeletonArgs, MctServer};
use rmcp::handler::server::wrapper::Parameters;

#[path = "common/content.rs"]
mod content;

#[tokio::test]
async fn gdscript_works_through_the_production_registry_and_index() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gdscript-app");
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let registry = mct_languages::build_registry();
    index.reindex(&registry, true).unwrap();

    let player = index.find_symbol("WorkshopPlayer").unwrap();
    assert_eq!(player.len(), 1);
    assert_eq!(player[0].language, "gdscript");

    let singleton = index.find_symbol("WorkshopState").unwrap();
    assert_eq!(singleton.len(), 1);
    assert_eq!(singleton[0].relative_path, "game/project.godot");
    let autoloads = index.dependency_graph("game/project.godot", 1, 20).unwrap();
    assert!(
        autoloads
            .edges
            .iter()
            .any(|edge| edge.to == "game/scripts/helper.gd"),
        "{autoloads:?}"
    );

    let graph = index.dependency_graph("game/player.gd", 1, 20).unwrap();
    assert!(graph.external.is_empty(), "{graph:?}");
    for path in ["game/scripts/base.gd", "game/scripts/helper.gd"] {
        assert!(graph.edges.iter().any(|edge| edge.to == path), "{graph:?}");
    }

    let callers = index.find_callers("move").unwrap();
    assert!(
        callers
            .iter()
            .any(|hit| hit.from_symbol == "play" && hit.resolution == Resolution::Resolved),
        "{callers:?}"
    );
    let constructors = index.find_callers("_init").unwrap();
    assert!(
        constructors
            .iter()
            .any(|hit| hit.from_symbol == "_init" && hit.resolution == Resolution::Resolved),
        "{constructors:?}"
    );
    assert!(
        constructors
            .iter()
            .any(|hit| hit.from_symbol == "play" && hit.resolution == Resolution::Resolved),
        "{constructors:?}"
    );
    let literals = index
        .search_literals("Workshop player movement", QueryScope::default())
        .unwrap();
    assert_eq!(literals.len(), 1);

    let status = index.status().unwrap();
    assert!(
        status.dependencies.iter().any(|manifest| {
            manifest.manifest_path == "game/project.godot"
                && manifest.language == "gdscript"
                && manifest
                    .dependencies
                    .iter()
                    .any(|dep| dep.name == "res://addons/workshop/plugin.cfg")
        }),
        "{status:?}"
    );

    let http = index.find_callers("workshop").unwrap();
    assert!(
        http.iter()
            .any(|hit| hit.from_symbol == "play" && hit.resolution == Resolution::Resolved),
        "{http:?}"
    );

    let duplicates = mct_index::find_duplicate_groups(
        &index,
        &mct_languages::build_token_sources(),
        mct_index::DuplicateOptions {
            language: Some("gdscript".into()),
            min_tokens: 2,
            min_lines: 2,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(duplicates.complete, "{:?}", duplicates.skipped);
    assert!(duplicates.candidates_analyzed >= 4);
    assert!(!duplicates.groups.is_empty());
    assert!(duplicates
        .supported_languages
        .iter()
        .any(|language| language == "gdscript"));

    let server = MctServer::new(index, registry);
    let skeleton = server
        .get_file_skeleton(Parameters(GetFileSkeletonArgs {
            path: "game/player.gd".into(),
        }))
        .await
        .unwrap();
    let text = content::content_of(&skeleton);
    assert!(text.contains("class_name WorkshopPlayer"), "{text}");
    assert!(text.contains("func play(steps: int = 2) -> void"), "{text}");
    assert!(!text.contains("self.move(steps)"), "{text}");
}

#[path = "common/tempdir.rs"]
mod tempdir;

#[test]
fn an_incremental_edit_replaces_autoload_and_plugin_rows() {
    let root = tempdir::tempdir("mct-godot-autoload-");
    let manifest = root.join("project.godot");
    std::fs::write(
        &manifest,
        include_str!("fixtures/gdscript-app/game/project.godot"),
    )
    .unwrap();

    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let registry = mct_languages::build_registry();
    index.reindex(&registry, false).unwrap();
    assert_eq!(index.find_symbol("WorkshopState").unwrap().len(), 1);

    std::fs::write(
        &manifest,
        "[autoload]\nNewState=\"*res://scripts/helper.gd\"\n[editor_plugins]\nenabled=PackedStringArray()\n",
    ).unwrap();
    index.reindex_paths(&registry, &[manifest]).unwrap();
    assert!(index.find_symbol("WorkshopState").unwrap().is_empty());
    assert_eq!(index.find_symbol("NewState").unwrap().len(), 1);
    assert!(index.status().unwrap().dependencies.is_empty());

    drop(index);
    std::fs::remove_dir_all(root).unwrap();
}
