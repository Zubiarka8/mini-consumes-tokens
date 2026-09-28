//! Exercises `mct_mcp_server::background::spawn_watcher` directly against a
//! real temp directory and real filesystem events (notify has no mockable
//! clock, so this uses a short debounce and a small real sleep — the
//! accepted pattern for notify-based tests).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::background;
use tokio::sync::Mutex;

/// A fresh temp directory, matching the hand-rolled convention already used
/// by `crates/mct-index/tests/reindex.rs` (no `tempfile` dependency needed
/// for a single throwaway dir per test). Canonicalized because on macOS
/// `std::env::temp_dir()` is under `/var/folders/...`, a symlink to
/// `/private/var/folders/...` — FSEvents reports the resolved path, so the
/// watcher's `strip_prefix` against an uncanonicalized root would never
/// match and every event would be (wrongly) filtered out as irrelevant.
fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-mcp-server-test-{}", uuid_like()));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn uuid_like() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}

const DEBOUNCE: Duration = Duration::from_millis(100);
/// Real filesystem watcher backends (FSEvents/inotify/ReadDirectoryChangesW)
/// add their own OS-level latency on top of the debounce timeout, so tests
/// give it a generous window rather than racing the debounce value exactly.
/// 5s proved too tight on shared/loaded `ubuntu-latest` GitHub Actions
/// runners (inotify event delivery can lag well past the debounce timeout
/// under CPU contention) — 20s keeps the same "poll until true" shape while
/// giving CI enough headroom not to flake.
const SETTLE_MARGIN: Duration = Duration::from_secs(20);

/// Polls `condition` until it's true or `SETTLE_MARGIN` (from `start`)
/// elapses — a single fixed sleep before asserting "it happened" is flaky
/// against real OS watcher latency, so this retries instead.
async fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + SETTLE_MARGIN;
    loop {
        if condition() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn watcher_reindexes_after_a_file_change_settles() {
    let dir = tempdir();
    fs::write(dir.join("lib.rs"), "pub fn one() {}\n").unwrap();

    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    assert!(index.find_symbol("two").unwrap().is_empty());

    let index = Arc::new(Mutex::new(index));
    let _watcher = background::spawn_watcher(
        Arc::clone(&index),
        registry,
        dir.clone(),
        ExcludeSet::default(),
        DEBOUNCE,
    )
    .unwrap();

    fs::write(dir.join("lib.rs"), "pub fn one() {}\npub fn two() {}\n").unwrap();

    let found = wait_until(|| {
        index
            .try_lock()
            .map(|guard| !guard.find_symbol("two").unwrap().is_empty())
            .unwrap_or(false)
    })
    .await;
    assert!(
        found,
        "expected the watcher to have auto-reindexed and picked up `two`"
    );
}

#[tokio::test]
async fn a_change_under_an_excluded_path_does_not_trigger_a_reindex() {
    let dir = tempdir();
    fs::write(dir.join("lib.rs"), "pub fn one() {}\n").unwrap();
    fs::create_dir_all(dir.join("node_modules")).unwrap();

    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let last_indexed_before = index.status().unwrap().last_indexed_at;

    let index = Arc::new(Mutex::new(index));
    let _watcher = background::spawn_watcher(
        Arc::clone(&index),
        registry,
        dir.clone(),
        ExcludeSet::default(),
        DEBOUNCE,
    )
    .unwrap();

    // node_modules/** is excluded by default (crates/mct-index/src/exclude.rs)
    // — a change only here must never trigger a reindex, or writing the
    // index's own `.mct-index/` database back to itself would loop forever.
    fs::write(dir.join("node_modules/x.js"), "function ignored() {}\n").unwrap();
    tokio::time::sleep(SETTLE_MARGIN).await;

    let last_indexed_after = index.lock().await.status().unwrap().last_indexed_at;
    assert_eq!(
        last_indexed_before, last_indexed_after,
        "an excluded-only change must not trigger an auto-reindex"
    );
}

#[tokio::test]
async fn watcher_drops_a_deleted_file_and_follows_a_rename_incrementally() {
    let dir = tempdir();
    fs::write(dir.join("keep.rs"), "pub fn keep() {}\n").unwrap();
    fs::write(dir.join("gone.rs"), "pub fn gone() {}\n").unwrap();
    fs::write(dir.join("old.rs"), "pub fn moved() {}\n").unwrap();

    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();

    let index = Arc::new(Mutex::new(index));
    let _watcher = background::spawn_watcher(
        Arc::clone(&index),
        registry,
        dir.clone(),
        ExcludeSet::default(),
        DEBOUNCE,
    )
    .unwrap();

    fs::remove_file(dir.join("gone.rs")).unwrap();
    fs::rename(dir.join("old.rs"), dir.join("new.rs")).unwrap();

    let settled = wait_until(|| {
        index
            .try_lock()
            .map(|guard| {
                guard.find_symbol("gone").unwrap().is_empty()
                    && guard
                        .find_symbol("moved")
                        .unwrap()
                        .iter()
                        .map(|h| h.relative_path.as_str())
                        .eq(["new.rs"])
            })
            .unwrap_or(false)
    })
    .await;
    assert!(
        settled,
        "expected the deletion and the rename to be indexed"
    );
    // An unrelated file is untouched (its function and its file-level module).
    assert_eq!(index.lock().await.find_symbol("keep").unwrap().len(), 2);
}

mod changed_paths {
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use mct_index::ExcludeSet;
    use mct_mcp_server::background::{changed_paths, Changed, MAX_INCREMENTAL_PATHS};
    use notify::event::{AccessKind, CreateKind, Flag, ModifyKind, RemoveKind};
    use notify::{Event, EventKind};
    use notify_debouncer_full::DebouncedEvent;

    fn event(kind: EventKind, paths: &[&str]) -> DebouncedEvent {
        let event = paths
            .iter()
            .fold(Event::new(kind), |e, p| e.add_path(PathBuf::from(p)));
        DebouncedEvent::new(event, Instant::now())
    }

    fn changed(events: &[DebouncedEvent]) -> Changed {
        changed_paths(events, Path::new("/repo"), &ExcludeSet::default())
    }

    #[test]
    fn collects_every_changed_path_once() {
        let events = [
            event(EventKind::Modify(ModifyKind::Any), &["/repo/src/a.rs"]),
            event(EventKind::Create(CreateKind::File), &["/repo/src/b.rs"]),
            event(EventKind::Remove(RemoveKind::File), &["/repo/src/a.rs"]),
            // A rename carries both ends.
            event(
                EventKind::Modify(ModifyKind::Any),
                &["/repo/old.rs", "/repo/new.rs"],
            ),
        ];
        assert_eq!(
            changed(&events),
            Changed::Paths(
                [
                    "/repo/new.rs",
                    "/repo/old.rs",
                    "/repo/src/a.rs",
                    "/repo/src/b.rs"
                ]
                .map(PathBuf::from)
                .to_vec()
            )
        );
    }

    #[test]
    fn reads_excluded_and_outside_paths_are_nothing() {
        let events = [
            event(EventKind::Access(AccessKind::Any), &["/repo/src/a.rs"]),
            event(EventKind::Modify(ModifyKind::Any), &["/repo/target/x.rs"]),
            event(
                EventKind::Modify(ModifyKind::Any),
                &["/repo/.mct-index/index.sqlite3"],
            ),
            event(EventKind::Modify(ModifyKind::Any), &["/elsewhere/a.rs"]),
        ];
        assert_eq!(changed(&events), Changed::Nothing);
    }

    #[test]
    fn a_rescan_request_or_a_huge_batch_is_a_full_reindex() {
        let mut rescan = event(EventKind::Other, &[]);
        rescan.event = rescan.event.clone().set_flag(Flag::Rescan);
        assert_eq!(changed(&[rescan]), Changed::Rescan);

        let many: Vec<DebouncedEvent> = (0..=MAX_INCREMENTAL_PATHS)
            .map(|i| {
                event(
                    EventKind::Modify(ModifyKind::Any),
                    &[&format!("/repo/f{i}.rs")],
                )
            })
            .collect();
        assert_eq!(changed(&many), Changed::Rescan);
    }
}
