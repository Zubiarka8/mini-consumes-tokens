//! One background maintainer per index (ADR-028): of two servers on one
//! project, exactly one watches; the other takes over when it stops, and
//! sees the owner's publications meanwhile.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::maintainer::{self, Maintainer, MaintainerSettings};
use tokio::sync::Mutex;

fn open(root: &Path) -> Arc<Mutex<Index>> {
    let db = root.join(".mct-index").join("index.sqlite3");
    let mut index = Index::open(root, &db, ExcludeSet::default()).unwrap();
    index
        .reindex(&mct_mcp_server::registry::build_registry(), false)
        .unwrap();
    Arc::new(Mutex::new(index))
}

fn start(index: &Arc<Mutex<Index>>, root: &Path) -> Maintainer {
    start_with_idle(index, root, None)
}

fn start_with_idle(index: &Arc<Mutex<Index>>, root: &Path, idle: Option<Duration>) -> Maintainer {
    maintainer::spawn(
        Arc::clone(index),
        mct_mcp_server::registry::build_registry(),
        MaintainerSettings {
            root: root.to_path_buf(),
            exclude: ExcludeSet::default(),
            debounce: Duration::from_millis(50),
            reconciliation_interval: Duration::from_secs(300),
            lease_retry: Duration::from_millis(100),
            idle,
        },
        maintainer::Activity::new(),
    )
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(30) {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn finds(index: &Arc<Mutex<Index>>, name: &str) -> bool {
    index
        .try_lock()
        .is_ok_and(|guard| guard.find_symbol(name).is_ok_and(|s| !s.is_empty()))
}

/// Removes the project directory when the test ends.
struct Project(PathBuf);

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn project() -> (Project, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mct-maintainer-{}-{serial}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let root = dir.canonicalize().unwrap();
    fs::write(root.join("one.rs"), "pub fn one() {}\n").unwrap();
    (Project(root.clone()), root)
}

#[test]
fn one_owner_watches_and_a_follower_takes_over_when_it_stops() {
    let (_dir, root) = project();
    let first_index = open(&root);
    let second_index = open(&root);
    let first = start(&first_index, &root);
    assert!(wait_until(|| first.is_owner()));
    let second = start(&second_index, &root);
    std::thread::sleep(Duration::from_millis(400));
    assert!(!second.is_owner(), "two processes maintain one index");

    // The owner indexes the edit; the follower serves it from the same file.
    fs::write(root.join("two.rs"), "pub fn two() {}\n").unwrap();
    assert!(wait_until(|| finds(&first_index, "two")));
    assert!(wait_until(|| finds(&second_index, "two")));

    drop(first);
    assert!(wait_until(|| second.is_owner()), "no takeover");
    fs::write(root.join("three.rs"), "pub fn three() {}\n").unwrap();
    assert!(wait_until(|| finds(&second_index, "three")));
}

#[test]
fn a_change_while_nobody_watched_is_caught_up_on_takeover() {
    let (_dir, root) = project();
    let first_index = open(&root);
    let second_index = open(&root);
    let first = start(&first_index, &root);
    assert!(wait_until(|| first.is_owner()));
    let second = start(&second_index, &root);
    std::thread::sleep(Duration::from_millis(200));

    // Owner gone; the edit lands before the follower's next attempt.
    drop(first);
    fs::write(root.join("gap.rs"), "pub fn in_the_gap() {}\n").unwrap();
    assert!(wait_until(|| second.is_owner()));
    assert!(wait_until(|| finds(&second_index, "in_the_gap")));
}

fn generation(index: &Arc<Mutex<Index>>) -> u64 {
    index.blocking_lock().published_generation().unwrap()
}

const IDLE: Duration = Duration::from_millis(300);

#[test]
fn an_idle_owner_sleeps_and_the_first_request_sees_changes_made_meanwhile() {
    let (_dir, root) = project();
    let index = open(&root);
    let owner = start_with_idle(&index, &root, Some(IDLE));
    let activity = owner.activity();
    assert!(wait_until(|| owner.is_owner()));
    assert!(wait_until(|| activity.is_asleep()), "never slept");
    assert!(!owner.is_owner(), "a sleeping process keeps the lease");

    // Edits while asleep: nothing watches, nothing is published.
    let before = generation(&index);
    fs::write(root.join("asleep.rs"), "pub fn written_while_asleep() {}\n").unwrap();
    fs::remove_file(root.join("one.rs")).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(generation(&index), before);

    // The first request waits for one reconciliation, then sees both.
    activity.wake_blocking().unwrap();
    let guard = index.blocking_lock();
    assert!(!guard
        .find_symbol("written_while_asleep")
        .unwrap()
        .is_empty());
    assert!(guard.find_symbol("one").unwrap().is_empty());
    drop(guard);
    assert!(
        wait_until(|| owner.is_owner()),
        "the waking process did not resume"
    );
}

#[test]
fn concurrent_wakes_share_one_refresh() {
    let (_dir, root) = project();
    let index = open(&root);
    let owner = start_with_idle(&index, &root, Some(IDLE));
    let activity = owner.activity();
    assert!(wait_until(|| activity.is_asleep()));
    let before = generation(&index);
    fs::write(root.join("two.rs"), "pub fn two() {}\n").unwrap();

    let waiters: Vec<_> = (0..8)
        .map(|_| {
            let activity = activity.clone();
            std::thread::spawn(move || activity.wake_blocking())
        })
        .collect();
    for waiter in waiters {
        waiter.join().unwrap().unwrap();
    }
    assert_eq!(generation(&index), before + 1, "more than one refresh");
    assert!(wait_until(|| finds(&index, "two")));
}

#[test]
fn a_sleeping_owner_hands_the_index_to_an_active_process() {
    let (_dir, root) = project();
    let sleepy_index = open(&root);
    let active_index = open(&root);
    let sleepy = start_with_idle(&sleepy_index, &root, Some(IDLE));
    assert!(wait_until(|| sleepy.is_owner()));
    let active = start(&active_index, &root);
    assert!(wait_until(|| sleepy.activity().is_asleep()));
    assert!(
        wait_until(|| active.is_owner()),
        "no takeover from a sleeper"
    );

    fs::write(root.join("three.rs"), "pub fn three() {}\n").unwrap();
    assert!(wait_until(|| finds(&active_index, "three")));
    // The sleeper wakes as a follower and answers from the shared index.
    sleepy.activity().wake_blocking().unwrap();
    assert!(wait_until(|| finds(&sleepy_index, "three")));
    assert!(!sleepy.is_owner());
}
