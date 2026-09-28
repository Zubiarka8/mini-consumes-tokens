//! Silent, debounced auto-reindexing: a filesystem watcher that keeps the
//! index fresh while the server runs, without requiring an agent to notice
//! staleness and call the `reindex` tool manually.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{new_debouncer, Debouncer, RecommendedCache};
use tokio::sync::Mutex;

/// `path`'s location relative to `root`, using forward slashes so it can be
/// checked against [`ExcludeSet`] the same way the indexer's own walk does.
/// Returns `None` for a path notify reports outside `root` — shouldn't
/// happen for a recursive watch rooted there, but never assumed.
fn relative_slash_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// Above this many distinct changed paths in one settled batch (a branch
/// switch, a mass rename, a generator run), one full walk is cheaper than a
/// per-path update of each.
pub const MAX_INCREMENTAL_PATHS: usize = 256;

/// What a settled batch of watcher events asks the index to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Changed {
    /// Nothing indexable changed (reads, excluded paths only).
    Nothing,
    /// Update just these absolute paths ([`Index::reindex_paths`]).
    Paths(Vec<PathBuf>),
    /// Walk everything ([`Index::reindex`]): the platform dropped events and
    /// asked for a rescan, or too many paths changed at once.
    Rescan,
}

/// Distills a debounced batch into [`Changed`]: every path of every
/// non-`Access` event that lies under `root` and outside `exclude`,
/// deduplicated. A rename contributes both its old and new path, so the old
/// one is dropped from the index and the new one indexed.
pub fn changed_paths(
    events: &[notify_debouncer_full::DebouncedEvent],
    root: &Path,
    exclude: &ExcludeSet,
) -> Changed {
    if events.iter().any(|event| event.need_rescan()) {
        return Changed::Rescan;
    }
    let mut paths: Vec<PathBuf> = events
        .iter()
        .filter(|event| !matches!(event.kind, EventKind::Access(_)))
        .flat_map(|event| event.paths.iter())
        .filter(|path| {
            relative_slash_path(root, path).is_some_and(|rel| !exclude.is_excluded(&rel))
        })
        .cloned()
        .collect();
    paths.sort();
    paths.dedup();
    match paths.len() {
        0 => Changed::Nothing,
        n if n > MAX_INCREMENTAL_PATHS => Changed::Rescan,
        _ => Changed::Paths(paths),
    }
}

/// Watches `root` recursively and, whenever the debouncer reports a settled
/// batch of changes outside `exclude`, updates the index for just the
/// changed paths ([`Index::reindex_paths`], issue #25). Falls back to a full
/// non-forced [`Index::reindex`] when the platform asks for a rescan, the
/// watcher reports errors (events may be lost) or the batch is huge.
/// `exclude` is the same [`ExcludeSet`] the indexer's own walk uses, so
/// writes to `.mct-index/` (the reindex's own database) never re-trigger
/// themselves into a loop.
///
/// Filters out `EventKind::Access` events (a plain open/read, not a
/// content change): `reindex` itself opens every indexed file to parse it,
/// and inotify reports that open back through the same watch, so without
/// this filter every reindex would trigger another one indefinitely.
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
) -> notify::Result<Debouncer<notify::RecommendedWatcher, RecommendedCache>> {
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(debounce, None, tx)?;
    debouncer.watch(&root, RecursiveMode::Recursive)?;

    std::thread::spawn(move || {
        for result in rx {
            let changed = match result {
                Ok(events) => {
                    tracing::debug!(
                        count = events.len(),
                        ?events,
                        "watcher received a debounced batch"
                    );
                    changed_paths(&events, &root, &exclude)
                }
                Err(errors) => {
                    for err in errors {
                        tracing::warn!(error = %err, "file watcher error; will keep watching");
                    }
                    // Events may have been lost: only a full walk is sure to
                    // catch whatever they described.
                    Changed::Rescan
                }
            };
            let result = match changed {
                Changed::Nothing => continue,
                Changed::Paths(paths) => index
                    .blocking_lock()
                    .reindex_paths(&registry, &paths)
                    .map(|report| ("incremental", report)),
                Changed::Rescan => index
                    .blocking_lock()
                    .reindex(&registry, false)
                    .map(|report| ("full", report)),
            };
            match result {
                Ok((mode, report)) => tracing::debug!(
                    mode,
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
