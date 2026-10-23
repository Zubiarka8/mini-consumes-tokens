//! Process-level test of the review store's writer lock. A separate process
//! takes the operating-system lock on `review-decisions.lock` and holds it. The
//! test checks, deterministically and on every platform, that:
//!
//! 1. while that process lives it owns the lock exclusively, so a second
//!    writer is refused with `ReviewError::Locked` and writes nothing;
//! 2. force-terminating the holder releases the lock without any cleanup, and
//!    the lock file it leaves behind stays empty and blocks nothing;
//! 3. the next save succeeds, and every decision acknowledged before the
//!    holder started is still there afterwards.
//!
//! Unlike `review_crash.rs`, which kills a saving writer at random moments,
//! this test kills a process that is known to hold the lock. The holder is this
//! same test binary, started again with environment variables and the test
//! name `lock_holder`; without the variables `lock_holder` does nothing. The
//! holder tells the test that it holds the lock by creating a marker file after
//! `try_lock` succeeded, and then waits to be killed. Every wait has a timeout.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mct_index::reviews::{Change, FindingKeys, Kind, ReviewError, Reviews, State};

const HOLDER_LOCK: &str = "MCT_REVIEW_LOCK_HOLDER_LOCK";
const HOLDER_READY: &str = "MCT_REVIEW_LOCK_HOLDER_READY";
const LOCK_FILE: &str = "review-decisions.lock";
const CANONICAL_FILE: &str = "review-decisions.toon";
const WAIT: Duration = Duration::from_secs(30);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}

fn finding(identity: char) -> FindingKeys {
    FindingKeys {
        kind: Kind::Duplicate,
        identity: identity.to_string().repeat(64),
        fingerprint: "b".repeat(64),
        content_known: true,
        legacy_key: None,
        draft_identity: "c".repeat(64),
    }
}

fn change(reviews: &Reviews, finding: &FindingKeys, state: State) -> Change {
    Change {
        state,
        note: String::new(),
        expected_revision: reviews.resolve(std::slice::from_ref(finding))[0].revision(),
    }
}

/// The lock holder: takes the lock, announces it and waits to be killed. Run
/// only by `a_killed_lock_holder_releases_the_lock`.
#[test]
fn lock_holder() {
    let (Some(lock), Some(ready)) = (
        std::env::var_os(HOLDER_LOCK),
        std::env::var_os(HOLDER_READY),
    ) else {
        return;
    };
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock)
        .unwrap();
    file.try_lock().expect("the holder starts with a free lock");
    File::create(ready).unwrap();
    // Held until the test kills this process. Bounded, so an orphan ends by
    // itself if the test binary dies first.
    thread::sleep(WAIT * 2);
    drop(file);
}

fn start_holder(lock: &Path, ready: &Path) -> Child {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_holder", "--nocapture", "--test-threads=1"])
        .env(HOLDER_LOCK, lock)
        .env(HOLDER_READY, ready)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !ready.exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("the lock holder exited early: {status}");
        }
        if started.elapsed() > WAIT {
            child.kill().ok();
            child.wait().ok();
            panic!("the lock holder never announced the lock");
        }
        thread::sleep(Duration::from_millis(10));
    }
    child
}

#[test]
fn a_killed_lock_holder_releases_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let store = root.join(".mct-index");
    fs::create_dir(&store).unwrap();
    let lock = store.join(LOCK_FILE);
    let canonical = store.join(CANONICAL_FILE);

    // An acknowledged decision exists before the holder starts.
    let first = finding('a');
    let mut reviews = Reviews::load(&root);
    reviews
        .save(
            std::slice::from_ref(&first),
            0,
            &change(&reviews, &first, State::Confirmed),
            now_ms(),
        )
        .expect("the first save");
    let before = fs::read(&canonical).unwrap();

    let mut holder = start_holder(&lock, &store.join("holder.ready"));

    // 1. The holder owns the lock exclusively: a second writer is refused,
    // and nothing is written.
    let second = finding('d');
    let error = reviews
        .save(
            std::slice::from_ref(&second),
            0,
            &change(&reviews, &second, State::Dismissed),
            now_ms(),
        )
        .expect_err("a writer must be refused while another process holds the lock");
    assert!(matches!(error, ReviewError::Locked { .. }), "{error}");
    assert!(error.to_string().contains(LOCK_FILE), "{error}");
    let probe = OpenOptions::new().write(true).open(&lock).unwrap();
    assert!(
        matches!(probe.try_lock(), Err(fs::TryLockError::WouldBlock)),
        "the lock must be exclusive"
    );
    assert_eq!(
        fs::read(&canonical).unwrap(),
        before,
        "a refused save writes nothing"
    );
    drop(probe);

    // 2. Forced termination: no cleanup runs in the holder.
    holder.kill().unwrap();
    holder.wait().unwrap();
    assert!(lock.exists(), "the lock file is never removed");
    assert_eq!(
        fs::metadata(&lock).unwrap().len(),
        0,
        "the lock file stays empty"
    );

    // 3. The operating system released the lock: the next save succeeds.
    reviews
        .save(
            std::slice::from_ref(&second),
            0,
            &change(&reviews, &second, State::Dismissed),
            now_ms(),
        )
        .expect("the lock is free once its holder was killed");

    // The decision acknowledged before the holder is preserved, with the new one.
    let reloaded = Reviews::load(&root);
    assert!(!reloaded.is_degraded(), "{:?}", reloaded.problems());
    assert_eq!(reloaded.resolve(&[first])[0].state(), State::Confirmed);
    assert_eq!(reloaded.resolve(&[second])[0].state(), State::Dismissed);
    let leftovers = fs::read_dir(&store)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .count();
    assert_eq!(leftovers, 0, "no temporary file remains");
}
