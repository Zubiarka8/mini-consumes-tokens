//! Live compatibility tests between this `mct-cli web` and an older one,
//! both saving review decisions in the same project at the same time.
//!
//! Ignored by default: they need the older binary, built from `e68aec9`
//! (`origin/main` before shared review decisions), in `MCT_COMPAT_MA`.
//! `scripts/unix/review-compat.sh` builds it from a `git archive` and runs:
//!
//! ```sh
//! MCT_COMPAT_MA=<path> cargo test -p mct-cli --test review_compat -- --ignored
//! ```
//!
//! Every project is a fresh temporary directory; no developer index or
//! review file is read or written.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use mct_index::reviews::Reviews;
use serde_json::{json, Value};

fn new_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mct-cli"))
}

fn old_binary() -> PathBuf {
    let path = std::env::var_os("MCT_COMPAT_MA").expect(
        "MCT_COMPAT_MA must name an mct-cli built from e68aec9; see scripts/unix/review-compat.sh",
    );
    PathBuf::from(path)
}

/// The dashboard fixture: one duplicate group and two dead-code candidates,
/// with its canonical path (the older binary refuses symlinked ancestors,
/// and macOS temporary directories live under one).
fn project(tag: &str) -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::Builder::new()
        .prefix(&format!("mct-review-compat-{tag}-"))
        .tempdir()
        .unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    let body = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";
    std::fs::write(root.path().join("src/a.rs"), format!("pub fn alpha{body}")).unwrap();
    std::fs::write(root.path().join("src/b.rs"), format!("pub fn beta{body}")).unwrap();
    let init = Command::new(new_binary())
        .arg("--root")
        .arg(root.path())
        .arg("init")
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let path = root.path().canonicalize().unwrap();
    (root, path)
}

/// A running `mct-cli web`, stopped on drop.
struct Web {
    child: Child,
    port: u16,
    token: String,
    csrf: String,
    /// Older dashboards take no `revision` field.
    sends_revision: bool,
}

impl Web {
    fn start(binary: &Path, root: &Path, sends_revision: bool) -> Self {
        let mut child = Command::new(binary)
            .arg("--root")
            .arg(root)
            .args(["web", "--port", "0", "--no-obsidian"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let started = Instant::now();
        let url = loop {
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "web did not start"
            );
            let line = lines
                .next()
                .expect("web exited before printing its URL")
                .unwrap();
            if let Some(url) = line.strip_prefix("Local project web: http://127.0.0.1:") {
                break url.to_string();
            }
        };
        let (port, token) = url.trim_end_matches('/').split_once('/').unwrap();
        let mut web = Self {
            child,
            port: port.parse().unwrap(),
            token: token.to_string(),
            csrf: String::new(),
            sends_revision,
        };
        web.csrf = web.get("api/meta")["csrf"].as_str().unwrap().to_string();
        web
    }

    fn request(&self, method: &str, path: &str, body: &str) -> (u16, Value) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        let origin = format!("http://127.0.0.1:{}", self.port);
        write!(
            stream,
            "{method} /{}/{path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: {origin}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.token,
            self.port,
            body.len()
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let status = response[9..12].parse().unwrap();
        let body = response.split_once("\r\n\r\n").map_or("", |(_, body)| body);
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    fn get(&self, path: &str) -> Value {
        let (status, body) = self.request("GET", path, "");
        assert_eq!(status, 200, "GET {path}: {body}");
        body
    }

    /// Keys of every finding of `kind`, in list order.
    fn keys(&self, kind: &str) -> Vec<String> {
        self.get(&format!("api/findings?type={kind}&size=100"))["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["key"].as_str().unwrap().to_string())
            .collect()
    }

    fn state(&self, key: &str) -> String {
        self.get(&format!("api/finding?key={key}"))["state"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// Posts a decision; this dashboard sends the revision it shows now,
    /// the older one sends none.
    fn decide(&self, key: &str, state: Option<&str>) -> u16 {
        let mut body = json!({ "csrf": self.csrf, "key": key, "state": state, "note": "" });
        if self.sends_revision {
            body["revision"] = self.get(&format!("api/finding?key={key}"))["revision"].clone();
        }
        self.request("POST", "api/review", &body.to_string()).0
    }

    /// Posts a decision based on `revision`, whatever is current.
    fn decide_from(&self, key: &str, state: &str, revision: &Value) -> u16 {
        let body = json!({ "csrf": self.csrf, "key": key, "state": state, "note": "", "revision": revision });
        self.request("POST", "api/review", &body.to_string()).0
    }
}

impl Drop for Web {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read(root: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(root.join(".mct-index").join(name)).ok()
}

/// CW-1, CR-5, CR-6: alternating saves on different findings by both
/// binaries are all kept; each binary still reads its own file; a rollback
/// and a re-upgrade lose nothing, and the latest decision wins.
#[test]
#[ignore = "needs MCT_COMPAT_MA; run scripts/unix/review-compat.sh"]
fn alternating_old_and_new_saves_are_all_kept() {
    let root = project("alternating");
    let new = Web::start(&new_binary(), root.1.as_path(), true);
    let old = Web::start(&old_binary(), root.1.as_path(), false);
    let (new_keys, old_keys) = (new.keys("dead-code"), old.keys("dead-code"));
    assert_eq!((new_keys.len(), old_keys.len()), (2, 2));
    let states = ["in-review", "confirmed", "dismissed", "false-positive"];
    for round in 0..50 {
        assert_eq!(new.decide(&new_keys[0], Some(states[round % 4])), 200);
        assert_eq!(old.decide(&old_keys[1], Some(states[(round + 1) % 4])), 200);
    }
    // 50 % 4 == 2: the last new state is states[1], the last old one states[2].
    let restarted = Web::start(&new_binary(), root.1.as_path(), true);
    assert_eq!(restarted.state(&new_keys[0]), "confirmed");
    assert_eq!(restarted.state(&new_keys[1]), "dismissed");
    let old_again = Web::start(&old_binary(), root.1.as_path(), false);
    assert_eq!(old_again.state(&old_keys[1]), "dismissed");
    assert_eq!(
        old_again.state(&old_keys[0]),
        "unreviewed",
        "the old binary ignores C3"
    );
    let canonical = read(root.1.as_path(), "review-decisions.toon").unwrap();
    assert!(canonical.starts_with("version: 3\n"));
    assert!(read(root.1.as_path(), "duplicate-reviews.toon")
        .unwrap()
        .starts_with("version: 2\n"));

    // Rollback: the old binary decides again about the new binary's finding,
    // in a later second than the last new save (equal seconds are a tie,
    // decided for the visible outcome).
    std::thread::sleep(Duration::from_millis(1100));
    assert_eq!(old_again.decide(&old_keys[0], Some("dismissed")), 200);
    assert_eq!(
        read(root.1.as_path(), "review-decisions.toon").unwrap(),
        canonical
    );
    // Re-upgrade: that later decision wins.
    let upgraded = Web::start(&new_binary(), root.1.as_path(), true);
    assert_eq!(upgraded.state(&new_keys[0]), "dismissed");
}

/// CW-2: the new dashboard refuses to overwrite a decision the old one made
/// after the page was loaded, then restores it from the current revision.
#[test]
#[ignore = "needs MCT_COMPAT_MA; run scripts/unix/review-compat.sh"]
fn a_restore_over_an_old_binary_decision_needs_the_current_revision() {
    let root = project("restore");
    let new = Web::start(&new_binary(), root.1.as_path(), true);
    let old = Web::start(&old_binary(), root.1.as_path(), false);
    let key = new.keys("duplicate")[0].clone();
    let shown = new.get(&format!("api/finding?key={key}"))["revision"].clone();
    assert_eq!(
        old.decide(&old.keys("duplicate")[0], Some("dismissed")),
        200
    );
    let legacy = read(root.1.as_path(), "duplicate-reviews.toon").unwrap();
    assert_eq!(new.decide_from(&key, "unreviewed", &shown), 409);
    assert_eq!(
        new.state(&key),
        "dismissed",
        "the conflict shows the old decision"
    );
    assert_eq!(new.decide(&key, None), 200);
    assert_eq!(new.state(&key), "unreviewed");
    assert_eq!(
        read(root.1.as_path(), "duplicate-reviews.toon").unwrap(),
        legacy
    );
    let history = new.get(&format!("api/finding?key={key}"))["history"].clone();
    let sources: Vec<&str> = history
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["source"].as_str().unwrap())
        .collect();
    assert_eq!(sources, ["a2", "c3"]);
    // The old binary keeps showing its own decision: documented divergence.
    let old_again = Web::start(&old_binary(), root.1.as_path(), false);
    assert_eq!(
        old_again.state(&old_again.keys("duplicate")[0]),
        "dismissed"
    );
}

/// CW-5: two new dashboards saving the same finding from the same view:
/// exactly one wins, the other gets 409, and history grows by one.
#[test]
#[ignore = "needs MCT_COMPAT_MA; run scripts/unix/review-compat.sh"]
fn two_new_writers_from_the_same_view_conflict_instead_of_overwriting() {
    let root = project("two-new");
    let first = Web::start(&new_binary(), root.1.as_path(), true);
    let second = Web::start(&new_binary(), root.1.as_path(), true);
    let key = first.keys("duplicate")[0].clone();
    let shown = first.get(&format!("api/finding?key={key}"))["revision"].clone();
    let results = [
        first.decide_from(&key, "dismissed", &shown),
        second.decide_from(&key, "confirmed", &shown),
    ];
    assert_eq!(results, [200, 409]);
    let history = Web::start(&new_binary(), root.1.as_path(), true)
        .get(&format!("api/finding?key={key}"))["history"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(history, 1);
}

/// CW-6: both binaries saving the same finding at the same time while a
/// reader keeps loading: no partial file is ever read, every request is
/// answered, and each file holds exactly its writer's acknowledged changes.
/// The two writers use disjoint states, so every acknowledged save changes
/// the state its writer sees and appends one history row.
#[test]
#[ignore = "needs MCT_COMPAT_MA; run scripts/unix/review-compat.sh"]
fn concurrent_old_and_new_writers_never_expose_a_partial_file() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let root = project("concurrent");
    let path = root.1.clone();
    let new = Web::start(&new_binary(), &path, true);
    let old = Web::start(&old_binary(), &path, false);
    let new_key = new.keys("dead-code")[0].clone();
    let old_key = old.keys("dead-code")[0].clone();
    let stop = Arc::new(AtomicBool::new(false));
    let reader = {
        let (path, stop) = (path.clone(), stop.clone());
        std::thread::spawn(move || {
            let mut loads = 0;
            while !stop.load(Ordering::Relaxed) {
                let reviews = Reviews::load(&path);
                assert!(!reviews.is_degraded(), "{:?}", reviews.problems());
                loads += 1;
            }
            loads
        })
    };
    let old_writer = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut saved = 0;
            while !stop.load(Ordering::Relaxed) {
                let state = ["dismissed", "false-positive"][saved % 2];
                assert_eq!(old.decide(&old_key, Some(state)), 200);
                saved += 1;
            }
            saved
        })
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let (mut new_saved, mut new_conflicts, mut round) = (0, 0, 0);
    while Instant::now() < deadline {
        match new.decide(&new_key, Some(["in-review", "confirmed"][round % 2])) {
            200 => new_saved += 1,
            409 => new_conflicts += 1,
            other => panic!("new dashboard answered {other}"),
        }
        round += 1;
    }
    stop.store(true, Ordering::Relaxed);
    let loads = reader.join().unwrap();
    let old_saved = old_writer.join().unwrap();
    assert!(loads > 0 && new_saved > 0 && old_saved > 0);
    let history_rows = |name: &str| {
        read(&path, name)
            .unwrap()
            .lines()
            .skip_while(|line| !line.starts_with("history["))
            .skip(1)
            .count()
    };
    // A conflict-free save of a state the new writer last saw unchanged is
    // a no-op; alternating states rules that out, conflicts aside.
    let canonical = history_rows("review-decisions.toon");
    assert!(
        canonical <= new_saved && canonical + new_conflicts >= new_saved,
        "{canonical} rows, {new_saved} saves, {new_conflicts} conflicts"
    );
    assert_eq!(history_rows("duplicate-reviews.toon"), old_saved);
    let final_state = Web::start(&new_binary(), &path, true).state(&new_key);
    assert!(
        ["in-review", "confirmed", "dismissed", "false-positive"].contains(&final_state.as_str()),
        "{final_state}"
    );
    eprintln!("CW-6: {new_saved} new saves, {new_conflicts} conflicts, {old_saved} old saves, {loads} reads");
}

/// CW-7: each binary's lock blocks only its own writer.
#[test]
#[ignore = "needs MCT_COMPAT_MA; run scripts/unix/review-compat.sh"]
fn each_lock_blocks_only_its_own_writer() {
    let root = project("locks");
    let new = Web::start(&new_binary(), root.1.as_path(), true);
    let old = Web::start(&old_binary(), root.1.as_path(), false);
    let lock = root.1.as_path().join(".mct-index/review-decisions.lock");
    std::fs::write(&lock, "held").unwrap();
    assert_eq!(
        old.decide(&old.keys("duplicate")[0], Some("confirmed")),
        200
    );
    assert_eq!(
        new.decide(&new.keys("dead-code")[0], Some("confirmed")),
        503
    );
    std::fs::remove_file(&lock).unwrap();
    let legacy_lock = root.1.as_path().join(".mct-index/duplicate-reviews.lock");
    std::fs::write(&legacy_lock, "held").unwrap();
    assert_eq!(
        new.decide(&new.keys("dead-code")[0], Some("confirmed")),
        200
    );
    assert_eq!(std::fs::read_to_string(legacy_lock).unwrap(), "held");
    assert!(!Reviews::load(&root.1).is_degraded());
}
