//! The atlas follows the normal refresh path but is never a context source.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::{
    background::{BatchState, Changed},
    server::MctServer,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

struct Project(PathBuf);
impl Project {
    fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("mct-atlas-server-{tag}-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

async fn call(server: &MctServer, tool: &str, args: Value) -> String {
    let result = server
        .call_tool_by_name(tool, args.as_object().cloned())
        .await
        .unwrap();
    assert_ne!(result.is_error, Some(true));
    result
        .content
        .iter()
        .filter_map(|c| c.as_text())
        .map(|c| c.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn mcp_reindex_warns_and_context_ignores_poisoned_vault_content() {
    let project = Project::new("mcp");
    fs::write(
        project.0.join("lib.rs"),
        "pub fn atlas_entry() { let source_marker = 42; }\n",
    )
    .unwrap();
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&project.0, ExcludeSet::default()).unwrap();
    mct_obsidian::register(&mut index);
    assert!(index.reindex(&registry, false).unwrap().warnings.is_empty());
    let vault = mct_obsidian::vault_dir(&project.0);
    fs::write(
        vault.join("index.md"),
        "# atlas_entry\n\nVAULT_POISON pub fn atlas_entry() { incorrect_context(); }",
    )
    .unwrap();
    let server = MctServer::new(index, registry);
    let report = call(&server, "reindex", json!({"force": false})).await;
    assert!(
        report.contains("warning:") && report.contains("modified vault file"),
        "{report}"
    );
    let files = server.index_handle().lock().await.indexed_files().unwrap();
    assert!(files
        .iter()
        .all(|file| !file.relative_path.starts_with(".mct-index/")));

    let context = call(
        &server,
        "build_context_pack",
        json!({"symbol": "atlas_entry", "path": "lib.rs"}),
    )
    .await;
    assert!(context.contains("source_marker"), "{context}");
    assert!(
        !context.contains("VAULT_POISON") && !context.contains("incorrect_context"),
        "{context}"
    );
    let symbols = call(
        &server,
        "list_symbols",
        json!({"path": ".mct-index/obsidian"}),
    )
    .await;
    assert!(!symbols.contains("atlas_entry"), "{symbols}");
}

#[test]
fn watcher_batches_use_the_registered_hook_and_exclude_generated_events() {
    use notify::{event::ModifyKind, Event, EventKind};
    let project = Project::new("batch");
    fs::write(project.0.join("lib.rs"), "pub fn initial() {}\n").unwrap();
    let registry = mct_mcp_server::registry::build_registry();
    let exclude = ExcludeSet::for_project(&project.0);
    let mut index = Index::open_in_memory(&project.0, exclude.clone()).unwrap();
    mct_obsidian::register(&mut index);
    let mut state = BatchState::default();
    let (_, report) = state
        .apply(&mut index, &registry, Changed::Rescan)
        .unwrap()
        .unwrap();
    assert!(report.warnings.is_empty());
    let vault = mct_obsidian::vault_dir(&project.0);
    assert!(vault.join("index.md").is_file());
    fs::write(project.0.join("lib.rs"), "pub fn changed() {}\n").unwrap();
    let (_, report) = state
        .apply(
            &mut index,
            &registry,
            Changed::Paths(vec![project.0.join("lib.rs")]),
        )
        .unwrap()
        .unwrap();
    assert!(report.warnings.is_empty());
    let manifest = mct_obsidian::Manifest::load(&vault).unwrap();
    assert!(manifest
        .notes
        .keys()
        .filter(|p| p.starts_with("files/"))
        .any(|p| fs::read_to_string(vault.join(p))
            .unwrap()
            .contains("`changed`")));
    let event = Event::new(EventKind::Modify(ModifyKind::Any)).add_path(vault.join("index.md"));
    let events = [notify_debouncer_full::DebouncedEvent::new(
        event,
        std::time::Instant::now(),
    )];
    assert_eq!(
        mct_mcp_server::background::changed_paths(&events, &project.0, &exclude),
        Changed::Nothing
    );
}
