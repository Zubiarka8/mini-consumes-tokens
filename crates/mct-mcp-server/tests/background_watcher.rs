//! Exercises `mct_mcp_server::background::spawn_watcher` directly against a
//! real temp directory and real filesystem events (notify has no mockable
//! clock, so this uses a short debounce and a small real sleep — the
//! accepted pattern for notify-based tests).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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
