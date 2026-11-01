//! The LuaRocks analysis (issue #267) as an MCP client sees it: declared,
//! installed and pinned through `get_indexing_status`, and the origin of
//! external `require`s through `get_dependency_graph`, with the real Lua
//! parser. The tree `manifest` is the one LuaRocks 3.13.0 wrote for
//! `inspect` and `lpeg`; the `luarocks.lock` is hand-written.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{GetDependencyGraphArgs, GetIndexingStatusArgs, MctServer};
use rmcp::handler::server::wrapper::Parameters;

const REAL_TREE: &str = include_str!(
    "../../mct-index/tests/fixtures/luarocks/real-tree/lib/luarocks/rocks-5.5/manifest"
);

/// A scratch project directory removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn tempdir() -> TempDir {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-luarocks-tools-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

fn content_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

fn write(root: &Path, path: &str, contents: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, contents).unwrap();
}

async fn server_for(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

fn project() -> TempDir {
    let dir = tempdir();
    let root = dir.path();
    write(
        root,
        "demo-1.0-1.rockspec",
        "package = 'demo'\nversion = '1.0-1'\n\
         dependencies = { 'lua >= 5.1', 'lpeg ~> 1.0', 'inspect >= 3.0', 'inspect < 4' }\n\
         test_dependencies = { 'busted' }\n",
    );
    write(
        root,
        "lua_modules/lib/luarocks/rocks-5.5/manifest",
        REAL_TREE,
    );
    write(
        root,
        "luarocks.lock",
        "return { dependencies = { inspect = '3.1.3-0', lpeg = '1.1.0-2' } }\n",
    );
    write(
        root,
        "app/main.lua",
        "local re = require(\"re\")\nlocal inspect = require(\"inspect\")\nlocal x = require(\"unknown.mod\")\n",
    );
    dir
}

#[tokio::test]
async fn the_status_tool_shows_declared_installed_and_pinned_per_dependency() {
    let dir = project();
    let server = server_for(dir.path()).await;
    let text = content_of(
        &server
            .get_indexing_status(Parameters(GetIndexingStatusArgs {
                verbose_dependencies: true,
                dependency: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("LuaRocks detail"), "got: {text}");
    // Both constraints on `inspect` are listed, not just the first.
    assert!(text.contains("[runtime] inspect >= 3.0"), "got: {text}");
    assert!(text.contains("[runtime] inspect < 4"), "got: {text}");
    assert!(text.contains("[test] busted"), "got: {text}");
    // `~> 1.0` is 1.0.x in LuaRocks, so the installed 1.1.0-2 does not meet it.
    assert!(text.contains("[runtime] lpeg ~> 1.0"), "got: {text}");
    assert!(
        text.contains("constraint: not met by 1.1.0-2"),
        "got: {text}"
    );
    assert!(
        text.contains("pinned: 1.1.0-2; installed: yes; meets constraint: no"),
        "got: {text}"
    );
    assert!(text.contains("constraint: met by 3.1.3-0"), "got: {text}");
    // Absence is qualified, never "not installed".
    assert!(
        text.contains("not found in the project-local trees (user and system trees not inspected)"),
        "got: {text}"
    );
    assert!(text.contains("pinned: not pinned"), "got: {text}");
    assert!(
        text.contains("effective version at run time"),
        "got: {text}"
    );
}

#[tokio::test]
async fn the_dependency_filter_narrows_the_detail_and_the_default_stays_cheap() {
    let dir = project();
    let server = server_for(dir.path()).await;
    let filtered = content_of(
        &server
            .get_indexing_status(Parameters(GetIndexingStatusArgs {
                verbose_dependencies: false,
                dependency: Some("lpeg".into()),
            }))
            .await
            .unwrap(),
    );
    assert!(
        filtered.contains("[runtime] lpeg ~> 1.0"),
        "got: {filtered}"
    );
    assert!(!filtered.contains("[runtime] inspect"), "got: {filtered}");

    let default = content_of(
        &server
            .get_indexing_status(Parameters(GetIndexingStatusArgs::default()))
            .await
            .unwrap(),
    );
    assert!(
        !default.contains("LuaRocks detail"),
        "no tree is read unless asked: {default}"
    );
}

#[tokio::test]
async fn the_graph_tool_relates_each_require_to_a_provider_a_declaration_or_nothing() {
    let dir = project();
    let server = server_for(dir.path()).await;
    let text = content_of(
        &server
            .get_dependency_graph(Parameters(GetDependencyGraphArgs {
                module: "app/main.lua".into(),
                depth: Some(1),
                limit: None,
                export: None,
                format: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("external import origins"), "got: {text}");
    // `re` is a module of the `lpeg` rock, which the rockspec declares.
    assert!(
        text.contains("provided by rock lpeg 1.1.0-2 (lua 5.5, lua_modules) via `re` - declared [runtime] ~> 1.0"),
        "got: {text}"
    );
    assert!(
        text.contains("sole candidate; every relevant source was read"),
        "got: {text}"
    );
    // `unknown.mod` has no provider: the declared rocks stay as candidates.
    assert!(text.contains("origin unknown"), "got: {text}");
    assert!(
        text.contains("declared rocks whose module lists are unknown"),
        "got: {text}"
    );
}

#[tokio::test]
async fn hostile_text_in_a_rockspec_cannot_inject_lines_into_the_output() {
    let dir = tempdir();
    write(
        dir.path(),
        "evil-1.0-1.rockspec",
        "dependencies = { 'lpeg >= 1.0\\n[runtime] injected\\u{1b}[31m' }\n",
    );
    let server = server_for(dir.path()).await;
    let text = content_of(
        &server
            .get_indexing_status(Parameters(GetIndexingStatusArgs {
                verbose_dependencies: true,
                dependency: None,
            }))
            .await
            .unwrap(),
    );
    assert!(
        !text.contains('\u{1b}'),
        "control characters must not reach the output"
    );
    assert!(!text.contains("\n[runtime] injected"), "got: {text:?}");
}

#[tokio::test]
async fn a_rockspec_over_the_read_limit_is_shown_as_possibly_stale_not_as_current() {
    let dir = tempdir();
    write(
        dir.path(),
        "big-1.0-1.rockspec",
        "dependencies = { 'lpeg >= 1.0' }\n",
    );
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(dir.path(), ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    // Past the 16 MiB read limit: the file is not read, the earlier data stays.
    write(
        dir.path(),
        "big-1.0-1.rockspec",
        &format!(
            "dependencies = {{ 'lpeg >= 1.0', 'other' }}\n-- {}",
            "x".repeat(17 * 1024 * 1024)
        ),
    );
    index.reindex(&registry, false).unwrap();
    let server = MctServer::new(index, registry);
    let text = content_of(
        &server
            .get_indexing_status(Parameters(GetIndexingStatusArgs {
                verbose_dependencies: true,
                dependency: None,
            }))
            .await
            .unwrap(),
    );
    assert!(
        text.contains("possibly stale: the rockspec could not be read on the last run"),
        "got: {text}"
    );
    assert!(text.contains("from an earlier read"), "got: {text}");
    assert!(text.contains("[runtime] lpeg >= 1.0"), "got: {text}");
    assert!(
        !text.contains("other"),
        "the unread edit must not appear: {text}"
    );
}
