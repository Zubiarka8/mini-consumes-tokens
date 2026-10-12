//! Black-box checks of `mct-mcp-server --reindex-delay <VALUE>` (issue #196):
//! the real binary over stdio, a real temp project and real filesystem events.
//!
//! Every test is `#[ignore]`d until the server gains the flag (branch
//! issue-196-server-delay-flag); on a base without it they fail because clap
//! rejects `--reindex-delay` as an unknown argument. Run them with
//! `scripts/unix/reindex-delay-smoke.sh`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior (see crates/mct-mcp-server/src/ for the
// production policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// Generous so a loaded CI runner does not flake the "appears eventually" checks.
const SETTLE_MARGIN: Duration = Duration::from_secs(30);

/// A fresh canonical temp project (macOS temp dirs are symlinks, and FSEvents
/// reports resolved paths).
fn tempdir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let dir = std::env::temp_dir().join(format!(
        "mct-reindex-delay-e2e-{}",
        nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
    ));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

struct StdioServer {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
}

impl StdioServer {
    fn start(root: &Path, extra_args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mct-mcp-server"))
            .arg("--root")
            .arg(root)
            .args(extra_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut server = Self {
            child,
            stdin,
            lines,
            next_id: 0,
        };
        let init = server.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "reindex-delay-e2e", "version": "0"}
            }),
        );
        assert!(init["error"].is_null(), "initialize failed: {init}");
        server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        server
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(120))
                .expect("the server answered in time");
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == json!(id) {
                return message;
            }
        }
    }

    /// Whether `find_symbol` currently reports `name`.
    fn finds(&mut self, name: &str) -> bool {
        let response = self.request(
            "tools/call",
            json!({"name": "find_symbol", "arguments": {"name": name}}),
        );
        response["result"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|block| block["text"].as_str())
            .any(|text| text.contains(name))
    }
}

impl Drop for StdioServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Runs the server expecting it to refuse its arguments; returns stderr.
fn run_rejected(args: &[&str]) -> String {
    let dir = tempdir();
    let output = Command::new(env!("CARGO_BIN_EXE_mct-mcp-server"))
        .arg("--root")
        .arg(&dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "{args:?} must exit non-zero, got {:?}",
        output.status
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
#[ignore = "needs issue-196-server-delay-flag"]
fn a_saved_file_becomes_searchable_only_after_the_delay() {
    let dir = tempdir();
    fs::write(dir.join("lib.rs"), "pub fn delay_one() {}\n").unwrap();
    let mut server = StdioServer::start(&dir, &["--reindex-delay", "500ms"]);
    assert!(server.finds("delay_one"), "the startup index must be ready");

    let saved = Instant::now();
    fs::write(
        dir.join("lib.rs"),
        "pub fn delay_one() {}\npub fn delay_two() {}\n",
    )
    .unwrap();
    // Not before the delay: asked right after the save, well inside 500 ms.
    if saved.elapsed() < Duration::from_millis(300) {
        assert!(
            !server.finds("delay_two"),
            "the file must not be indexed before the delay elapsed"
        );
    }
    let mut found = false;
    while saved.elapsed() < SETTLE_MARGIN {
        if server.finds("delay_two") {
            found = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(found, "expected the saved file to be indexed after the delay");
    assert!(
        saved.elapsed() >= Duration::from_millis(400),
        "indexed after only {:?}, before the 500ms delay",
        saved.elapsed()
    );
}

#[test]
#[ignore = "needs issue-196-server-delay-flag"]
fn off_never_auto_updates_a_saved_file() {
    let dir = tempdir();
    fs::write(dir.join("lib.rs"), "pub fn off_one() {}\n").unwrap();
    let mut server = StdioServer::start(&dir, &["--reindex-delay", "off"]);
    assert!(server.finds("off_one"), "the startup index must be ready");

    fs::write(
        dir.join("lib.rs"),
        "pub fn off_one() {}\npub fn off_two() {}\n",
    )
    .unwrap();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        assert!(
            !server.finds("off_two"),
            "`off` must never auto-index a saved file"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[test]
#[ignore = "needs issue-196-server-delay-flag"]
fn invalid_delay_values_are_rejected_with_a_clear_message() {
    for value in ["0s", "abc", "-5s", "10x"] {
        // `=` form so a leading `-` is read as the value, not a flag.
        let arg = format!("--reindex-delay={value}");
        let stderr = run_rejected(&[&arg]);
        assert!(
            stderr.contains("reindex-delay") && stderr.contains(value),
            "message for {value:?} must name the flag and the value: {stderr}"
        );
        assert!(
            !stderr.contains("unexpected argument") && !stderr.contains("unrecognized"),
            "{value:?} was rejected as an unknown flag, not as a bad value: {stderr}"
        );
    }
}

#[test]
#[ignore = "needs issue-196-server-delay-flag"]
fn reindex_delay_conflicts_with_the_debounce_flag() {
    let stderr = run_rejected(&["--reindex-delay", "30s", "--reindex-debounce-ms", "100"]);
    assert!(
        stderr.contains("cannot be used with") && stderr.contains("reindex-debounce-ms"),
        "expected a clap conflict message naming both flags: {stderr}"
    );
}
