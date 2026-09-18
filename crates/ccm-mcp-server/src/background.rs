//! Silent, debounced auto-reindexing: a filesystem watcher that keeps the
//! index fresh while the server runs, without requiring an agent to notice
//! staleness and call the `reindex` tool manually.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use ccm_core::LanguageRegistry;
use ccm_index::{ExcludeSet, Index};
use notify::RecursiveMode;
use notify_debouncer_mini::{DebouncedEventKind, Debouncer, new_debouncer};
use tokio::sync::Mutex;

/// `path`'s location relative to `root`, using forward slashes so it can be
/// checked against [`ExcludeSet`] the same way the indexer's own walk does.
/// Returns `None` for a path notify reports outside `root` — shouldn't
/// happen for a recursive watch rooted there, but never assumed.
fn relative_slash_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// Watches `root` recursively and reindexes (non-forced/incremental)
/// whenever the debouncer reports a settled batch of changes outside
/// `exclude` — the same [`ExcludeSet`] the indexer's own walk uses, so
/// writes to `.ccm-index/` (the reindex's own database) never re-trigger
/// themselves into a loop.
///
/// Returns the [`Debouncer`] guard — the caller must keep it alive for as
/// long as watching should continue; dropping it stops the watch and joins
/// its background thread.
pub fn spawn_watcher(
    index: Arc<Mutex<Index>>,
    registry: LanguageRegistry,
    root: PathBuf,
    exclude: ExcludeSet,
    debounce: Duration,
) -> notify::Result<Debouncer<notify::RecommendedWatcher>> {
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(debounce, tx)?;
    debouncer.watcher().watch(&root, RecursiveMode::Recursive)?;

    std::thread::spawn(move || {
        for result in rx {
            let events = match result {
                Ok(events) => events,
                Err(err) => {
                    tracing::warn!(error = %err, "file watcher error; will keep watching");
                    continue;
                }
            };
            tracing::debug!(count = events.len(), ?events, "watcher received a debounced batch");
            // TEMPORARY diagnostic instrumentation (2026-09-18): CI on
            // ubuntu-latest keeps failing background_watcher's
            // a_change_under_an_excluded_path_does_not_trigger_a_reindex with
            // an unexplained auto-reindex; dumping each event's raw path/kind
            // and its exclusion verdict to see what's actually slipping
            // through. Remove once root-caused.
            for event in &events {
                let rel = relative_slash_path(&root, &event.path);
                eprintln!(
                    "[DIAG] event.path={:?} rel={:?} kind={:?} excluded={:?}",
                    event.path,
                    rel,
                    event.kind,
                    rel.as_deref().map(|r| exclude.is_excluded(r))
                );
            }
            let relevant = events.iter().any(|event| {
                event.kind != DebouncedEventKind::AnyContinuous
                    && relative_slash_path(&root, &event.path)
                        .is_some_and(|rel| !exclude.is_excluded(&rel))
            });
            if !relevant {
                continue;
            }
            let mut guard = index.blocking_lock();
            match guard.reindex(&registry, false) {
                Ok(report) => tracing::debug!(
                    parsed = report.files_parsed,
                    unchanged = report.files_unchanged,
                    removed = report.files_removed,
                    "auto-reindex after file changes settled"
                ),
                Err(err) => {
                    tracing::warn!(error = %err, "auto-reindex failed; will retry on next change")
                }
            }
        }
    });

    Ok(debouncer)
}
