//! Silent, debounced auto-reindexing: a filesystem watcher that keeps the
//! index fresh while the server runs, without requiring an agent to notice
//! staleness and call the `reindex` tool manually.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, ReindexReport};
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
///
/// A batch touching one of the project's ignore files reloads `exclude`
/// ([`ExcludeSet::reload`] — shared with the index, so both switch rules
/// together) and, if the rules changed, is a [`Changed::Rescan`]: the paths
/// they newly exclude or un-exclude reported no event of their own.
pub fn changed_paths(
    events: &[notify_debouncer_full::DebouncedEvent],
    root: &Path,
    exclude: &ExcludeSet,
) -> Changed {
    if events.iter().any(|event| event.need_rescan()) {
        return Changed::Rescan;
    }
    let changes = || {
        events
            .iter()
            .filter(|event| !matches!(event.kind, EventKind::Access(_)))
            .flat_map(|event| event.paths.iter())
    };
    let rules_file_touched = changes().any(|path| {
        relative_slash_path(root, path).is_some_and(|rel| ExcludeSet::is_rules_file(&rel))
    });
    if rules_file_touched && exclude.reload() {
        return Changed::Rescan;
    }
    let mut paths: Vec<PathBuf> = changes()
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

/// The two ways a batch can bring the index up to date — [`Index`] itself,
/// or a stand-in that injects failures in tests.
pub trait Reindexer {
    fn reindex_paths(
        &mut self,
        registry: &LanguageRegistry,
        paths: &[PathBuf],
    ) -> mct_index::Result<ReindexReport>;
    fn reindex(&mut self, registry: &LanguageRegistry) -> mct_index::Result<ReindexReport>;
}

impl Reindexer for Index {
    fn reindex_paths(
        &mut self,
        registry: &LanguageRegistry,
        paths: &[PathBuf],
    ) -> mct_index::Result<ReindexReport> {
        Index::reindex_paths(self, registry, paths)
    }

    fn reindex(&mut self, registry: &LanguageRegistry) -> mct_index::Result<ReindexReport> {
        Index::reindex(self, registry, false)
    }
}

/// Longest wait before retrying a failed update when no new event arrives
/// (never shorter than the configured quiet period itself).
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);

/// What the watcher carries from one batch to the next. A failed update may
/// have applied part of its batch, and the paths it was given won't be
/// reported again until they change again — so after any failure the next
/// update is a full walk, whatever it was asked for, until one succeeds.
#[derive(Debug, Default)]
pub struct BatchState {
    failures: u32,
}

impl BatchState {
    /// Whether the last update failed and a full walk is owed.
    pub fn rescan_pending(&self) -> bool {
        self.failures > 0
    }

    /// How long to wait for an event before retrying the owed walk anyway:
    /// `debounce` doubled per consecutive failure, capped at a minute (or at
    /// `debounce` itself when that is longer — a retry never comes sooner
    /// than the quiet period), so a persistent failure (disk full, a locked database) doesn't spin.
    pub fn retry_delay(&self, debounce: Duration) -> Duration {
        let factor = 2u32.saturating_pow(self.failures.saturating_sub(1).min(16));
        debounce
            .saturating_mul(factor)
            .min(MAX_RETRY_DELAY.max(debounce))
    }

    /// Runs the update `changed` asks for (a full walk instead while one is
    /// owed) and records whether it succeeded. `None` when there was
    /// nothing to do.
    pub fn apply(
        &mut self,
        index: &mut impl Reindexer,
        registry: &LanguageRegistry,
        changed: Changed,
    ) -> Option<mct_index::Result<(&'static str, ReindexReport)>> {
        let changed = if self.rescan_pending() {
            Changed::Rescan
        } else {
            changed
        };
        let result = match changed {
            Changed::Nothing => return None,
            Changed::Paths(paths) => index
                .reindex_paths(registry, &paths)
                .map(|report| ("incremental", report)),
            Changed::Rescan => index.reindex(registry).map(|report| ("full", report)),
        };
        self.failures = match result {
            Ok(_) => 0,
            Err(_) => self.failures.saturating_add(1),
        };
        Some(result)
    }
}

/// Folds a newly classified batch into the changes already waiting for the
/// quiet period to end: a rescan absorbs everything, path sets are united,
/// and a union past [`MAX_INCREMENTAL_PATHS`] becomes a rescan.
pub fn merge_changed(pending: Changed, new: Changed) -> Changed {
    match (pending, new) {
        (Changed::Nothing, other) | (other, Changed::Nothing) => other,
        (Changed::Rescan, _) | (_, Changed::Rescan) => Changed::Rescan,
        (Changed::Paths(mut a), Changed::Paths(b)) => {
            a.extend(b);
            a.sort();
            a.dedup();
            if a.len() > MAX_INCREMENTAL_PATHS {
                Changed::Rescan
            } else {
                Changed::Paths(a)
            }
        }
    }
}

/// `notify_debouncer_full` only settles each file on its own (a file's
/// events are released once *that file* has been quiet for its timeout), so
/// it cannot by itself give a project-wide quiet period. It is therefore run
/// with at most this short timeout, merely to coalesce raw event noise, and
/// the watcher thread adds the rest of the configured delay as a quiet period
/// that every relevant batch restarts.
const MAX_COALESCE: Duration = Duration::from_millis(500);

/// Watches `root` recursively and, once no relevant change has arrived for
/// `debounce` (every new relevant change restarts the wait; a burst becomes
/// one update; irrelevant or excluded events never restart it),
/// updates the index for the changed paths outside `exclude`, updates the index for just the
/// changed paths ([`Index::reindex_paths`], issue #25). Falls back to a full
/// non-forced [`Index::reindex`] when the platform asks for a rescan, the
/// watcher reports errors (events may be lost), the batch is huge, or an
/// earlier update failed ([`BatchState`]) — retried after a backoff even
/// if nothing else changes.
/// `exclude` is the same [`ExcludeSet`] the indexer's own walk uses (a clone
/// of the index's handle, sharing its rules — edits to the project's ignore
/// files reload both at once, see [`changed_paths`]), so writes to
/// `.mct-index/` (the reindex's own database) never re-trigger themselves
/// into a loop.
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
    let coalesce = debounce.min(MAX_COALESCE);
    // Total latency after the last change stays ~`debounce`.
    let quiet = debounce - coalesce;
    let mut debouncer = new_debouncer(coalesce, None, tx)?;
    debouncer.watch(&root, RecursiveMode::Recursive)?;

    std::thread::spawn(move || {
        let mut state = BatchState::default();
        let mut pending = Changed::Nothing;
        let mut deadline = Instant::now();
        loop {
            let received = if pending != Changed::Nothing {
                match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(result) => Some(result),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            } else if state.rescan_pending() {
                match rx.recv_timeout(state.retry_delay(debounce)) {
                    Ok(result) => Some(result),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match rx.recv() {
                    Ok(result) => Some(result),
                    Err(_) => break,
                }
            };
            if let Some(result) = received {
                let batch = match result {
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
                        // Events may have been lost: only a full walk is sure
                        // to catch whatever they described.
                        Changed::Rescan
                    }
                };
                if batch != Changed::Nothing {
                    pending =
                        merge_changed(std::mem::replace(&mut pending, Changed::Nothing), batch);
                    deadline = Instant::now() + quiet;
                }
                if pending != Changed::Nothing && !quiet.is_zero() {
                    continue; // still inside the quiet period
                }
            }
            // Quiet period over (or a retry of an owed full walk is due).
            // Anything arriving while the update runs waits in the channel.
            let changed = std::mem::replace(&mut pending, Changed::Nothing);
            if changed == Changed::Nothing && !state.rescan_pending() {
                continue; // don't take the index lock for nothing
            }
            let mut guard = index.blocking_lock();
            match state.apply(&mut *guard, &registry, changed) {
                None => {}
                Some(Ok((mode, report))) => tracing::debug!(
                    mode,
                    parsed = report.files_parsed,
                    unchanged = report.files_unchanged,
                    removed = report.files_removed,
                    "auto-reindex after file changes settled"
                ),
                Some(Err(err)) => tracing::warn!(
                    error = %err,
                    "auto-reindex failed; a full reindex will retry it"
                ),
            }
        }
    });

    Ok(debouncer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Changed {
        Changed::Paths(names.iter().map(PathBuf::from).collect())
    }

    #[test]
    fn merge_groups_a_burst_into_one_deduplicated_set() {
        let merged = merge_changed(paths(&["/a", "/b"]), paths(&["/b", "/c"]));
        assert_eq!(merged, paths(&["/a", "/b", "/c"]));
    }

    #[test]
    fn merge_ignores_nothing_and_rescan_absorbs_everything() {
        assert_eq!(
            merge_changed(Changed::Nothing, paths(&["/a"])),
            paths(&["/a"])
        );
        assert_eq!(
            merge_changed(paths(&["/a"]), Changed::Nothing),
            paths(&["/a"])
        );
        assert_eq!(
            merge_changed(paths(&["/a"]), Changed::Rescan),
            Changed::Rescan
        );
        assert_eq!(
            merge_changed(Changed::Rescan, paths(&["/a"])),
            Changed::Rescan
        );
    }

    #[test]
    fn merge_escalates_to_rescan_past_the_incremental_cap() {
        let half = MAX_INCREMENTAL_PATHS / 2 + 1;
        let batch = |tag: &str| {
            Changed::Paths(
                (0..half)
                    .map(|i| PathBuf::from(format!("/{tag}{i}")))
                    .collect(),
            )
        };
        assert_eq!(merge_changed(batch("x"), batch("y")), Changed::Rescan);
    }

    #[test]
    fn retry_delay_never_undercuts_a_long_quiet_period() {
        let state = BatchState { failures: 1 };
        let five_minutes = Duration::from_secs(300);
        assert_eq!(state.retry_delay(five_minutes), five_minutes);
    }
}
