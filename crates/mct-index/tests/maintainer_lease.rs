//! One background maintainer per index: an OS lock separate from the writer
//! lock, released when its holder drops it or dies.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use mct_index::{ExcludeSet, Index};

const CHILD_ROOT: &str = "MCT_INDEX_LEASE_CHILD_ROOT";

fn database(root: &Path) -> PathBuf {
    root.join(".mct-index").join("index.sqlite3")
}

fn open(root: &Path) -> Index {
    Index::open(root, &database(root), ExcludeSet::default()).unwrap()
}

#[test]
fn exactly_one_index_holds_the_lease_until_it_drops_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let first = open(&root);
    let second = open(&root);

    let lease = first.try_acquire_maintainer().unwrap();
    assert!(lease.is_some());
    assert!(second.try_acquire_maintainer().unwrap().is_none());
    assert!(first.try_acquire_maintainer().unwrap().is_none());

    drop(lease);
    let taken = second.try_acquire_maintainer().unwrap();
    assert!(taken.is_some());
    assert!(first.try_acquire_maintainer().unwrap().is_none());
}

#[test]
fn the_lease_does_not_block_writers() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join("a.txt"), "x").unwrap();
    let owner = open(&root);
    let _lease = owner.try_acquire_maintainer().unwrap().unwrap();
    let mut follower = open(&root);
    follower
        .reindex(&mct_core::LanguageRegistry::new(), false)
        .unwrap();
}

#[test]
fn an_in_memory_index_is_always_its_own_maintainer() {
    let dir = tempfile::tempdir().unwrap();
    let index = Index::open_in_memory(dir.path(), ExcludeSet::default()).unwrap();
    let _one = index.try_acquire_maintainer().unwrap().unwrap();
    assert!(index.try_acquire_maintainer().unwrap().is_some());
}

#[test]
fn a_killed_holder_releases_the_lease() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    drop(open(&root));
    let ready = root.join("child-holds-lease");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("lease_holder_child")
        .arg("--nocapture")
        .env(CHILD_ROOT, &root)
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !ready.exists() {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "child never took the lease"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let index = open(&root);
    assert!(index.try_acquire_maintainer().unwrap().is_none());

    child.kill().unwrap();
    child.wait().unwrap();
    assert!(index.try_acquire_maintainer().unwrap().is_some());
}

/// Holds the lease until killed; only meaningful when spawned by the test
/// above, a no-op otherwise.
#[test]
fn lease_holder_child() {
    let Ok(root) = std::env::var(CHILD_ROOT) else {
        return;
    };
    let root = PathBuf::from(root);
    let index = open(&root);
    let _lease = index.try_acquire_maintainer().unwrap().unwrap();
    std::fs::write(root.join("child-holds-lease"), "").unwrap();
    loop {
        std::thread::park();
    }
}
