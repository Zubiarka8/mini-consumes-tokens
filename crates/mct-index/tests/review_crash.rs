//! Process-level crash test of the review store. A writer process saves
//! decisions in a loop and is killed with SIGKILL at varying moments, so the
//! kills land between saves, inside a save and after a save. After every kill
//! the store must load without problems, hold every save the writer
//! acknowledged, and report every temporary file a kill left behind.
//!
//! The writer is this same test binary, started again with environment
//! variables and the test name `writer`. The variables are unset in normal
//! runs, so `writer` does nothing then. Acknowledgements go to an append-only
//! file, one line per save, rather than a pipe: a killed process cannot lose
//! lines that it already wrote there.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mct_index::reviews::{Change, FindingKeys, Kind, Reviews, State};

const WRITER_ROOT: &str = "MCT_REVIEW_CRASH_WRITER_ROOT";
const WRITER_ACKS: &str = "MCT_REVIEW_CRASH_WRITER_ACKS";
const KILLS: usize = 150;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}

fn finding() -> FindingKeys {
    FindingKeys {
        kind: Kind::Duplicate,
        identity: "a".repeat(64),
        fingerprint: "b".repeat(64),
        content_known: true,
        legacy_key: None,
        draft_identity: "c".repeat(64),
    }
}

/// The writer process: saves until it is killed, appending one line per save
/// that returned. Run only by `review_store_survives_killed_writers`.
#[test]
fn writer() {
    let (Some(root), Some(acks)) = (std::env::var_os(WRITER_ROOT), std::env::var_os(WRITER_ACKS))
    else {
        return;
    };
    let mut reviews = Reviews::load(Path::new(&root));
    let findings = [finding()];
    let mut acks = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(acks)
        .unwrap();
    loop {
        // Repeating the current state writes nothing, so always change it: a
        // restarted writer must not repeat the last state of its predecessor.
        let state = if reviews.resolve(&findings)[0].state() == State::InReview {
            State::Confirmed
        } else {
            State::InReview
        };
        let revision = reviews.resolve(&findings)[0].revision();
        let change = Change {
            state,
            note: String::new(),
            expected_revision: revision,
        };
        reviews.save(&findings, 0, &change, now_ms()).expect("save");
        acks.write_all(b"saved\n").unwrap();
    }
}

/// Temporary files a killed save leaves in the store directory.
fn temporary_files(store: &Path) -> usize {
    std::fs::read_dir(store)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("review-decisions.toon.") && name.ends_with(".tmp")
        })
        .count()
}

/// Saves acknowledged by any writer so far.
fn acknowledged(acks: &Path) -> usize {
    std::fs::read_to_string(acks).map_or(0, |text| text.lines().count())
}

/// Starts the writer in `root`, lets it run for `delay` and kills it without
/// warning.
fn run_and_kill(root: &Path, acks: &Path, delay: Duration) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "writer", "--nocapture", "--test-threads=1"])
        .env(WRITER_ROOT, root)
        .env(WRITER_ACKS, acks)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    thread::sleep(delay);
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn review_store_survives_killed_writers() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let store = root.join(".mct-index");
    std::fs::create_dir(&store).unwrap();
    let acks = root.join("acks.log");
    let lock = store.join("review-decisions.lock");

    let mut interrupted_saves = 0;
    let mut stale_locks = 0;
    for run in 0..KILLS {
        // A kill never removes the lock; the message tells the person to remove
        // it by hand. The test does that, and counts how often it was needed.
        if lock.exists() {
            stale_locks += 1;
            std::fs::remove_file(&lock).unwrap();
        }
        // Delays from 3 ms to about 60 ms, so the kills move across the save loop.
        let delay = Duration::from_micros(3_000 + ((run * 7_919) % 57_000) as u64);
        run_and_kill(&root, &acks, delay);

        let reviews = Reviews::load(&root);
        assert!(
            !reviews.is_degraded(),
            "run {run}: {:?}",
            reviews.problems()
        );
        let persisted = reviews.histories(&[finding()]).remove(0).len();
        let acknowledged = acknowledged(&acks);
        // Each save is acknowledged after its rename, so every kill can leave at
        // most one save persisted but not yet acknowledged; the surplus adds up
        // over the kills so far.
        assert!(
            persisted >= acknowledged && persisted <= acknowledged + run + 1,
            "run {run}: {persisted} saves persisted, {acknowledged} acknowledged"
        );
        let temporaries = temporary_files(&store);
        assert_eq!(
            reviews.leftover_temporary_files(),
            temporaries,
            "run {run}: every leftover temporary file is reported"
        );
        interrupted_saves += usize::from(temporaries > 0);
        // The backup is the file before the last save: complete, never torn.
        if let Ok(backup) = std::fs::read_to_string(store.join("review-decisions.toon.prev")) {
            assert!(backup.starts_with("version: 3\n"), "run {run}");
            assert!(backup.ends_with('\n'), "run {run}: a torn backup");
        }
    }
    eprintln!(
        "{KILLS} kills, {} saves acknowledged: {interrupted_saves} left a temporary file, \
         {stale_locks} left a lock that had to be removed by hand",
        acknowledged(&acks)
    );
    assert!(interrupted_saves > 0, "no kill landed inside a save");

    // The store still accepts saves once the stale lock is gone.
    if lock.exists() {
        std::fs::remove_file(&lock).unwrap();
    }
    let mut reviews = Reviews::load(&root);
    let revision = reviews.resolve(&[finding()])[0].revision();
    let change = Change {
        state: State::Dismissed,
        note: String::new(),
        expected_revision: revision,
    };
    reviews
        .save(&[finding()], 0, &change, now_ms())
        .expect("the store accepts saves after the crashes");
    assert_eq!(
        Reviews::load(&root).resolve(&[finding()])[0].state(),
        State::Dismissed
    );
}
