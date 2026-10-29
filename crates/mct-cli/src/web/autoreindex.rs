//! Keeps the index fresh while the dashboard runs: a saved file, or an edited
//! `.mctignore`, shows up without a manual `reindex`. It is the MCP server's own
//! watcher ([`mct_mcp_server::background`]), so both apply the same rules.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use mct_index::{ExcludeSet, Index};

/// Quiet time after the last save before the index is rebuilt. Shorter than
/// the MCP server's 15 s: a person is watching this page for the change.
const DEBOUNCE: Duration = Duration::from_secs(2);

/// Starts watching `root` and brings a stale index up to date in the background.
/// The returned guard must stay alive: dropping it stops the watch. A failure
/// only turns automatic reindexing off, the dashboard itself keeps serving.
pub(super) fn start(root: &Path, no_obsidian: bool) -> Option<impl Sized> {
    match watch(root, no_obsidian) {
        Ok(guard) => Some(guard),
        Err(error) => {
            eprintln!("warning: automatic reindexing is off: {error:#}");
            None
        }
    }
}

fn watch(root: &Path, no_obsidian: bool) -> anyhow::Result<impl Sized> {
    // One set of rules shared by the index and the watcher, reloaded when
    // `.mctignore` or `.gitignore` change.
    let exclude = ExcludeSet::for_project(root);
    let mut index = Index::open(
        root,
        &root.join(".mct-index/index.sqlite3"),
        exclude.clone(),
    )?;
    if !no_obsidian {
        mct_obsidian::register_guarded(&mut index, mct_obsidian::PublishGuard::default());
    }
    let watch_root = index.root().to_path_buf();
    let index = Arc::new(tokio::sync::Mutex::new(index));
    let registry = mct_languages::build_registry();
    let watcher = mct_mcp_server::background::spawn_watcher(
        index.clone(),
        registry.clone(),
        watch_root,
        exclude,
        DEBOUNCE,
    )?;
    // Changes made while the dashboard was closed produce no event.
    std::thread::spawn(move || {
        if let Err(error) = index.blocking_lock().reindex(&registry, false) {
            eprintln!("warning: the startup reindex failed: {error:#}");
        }
    });
    Ok(watcher)
}
