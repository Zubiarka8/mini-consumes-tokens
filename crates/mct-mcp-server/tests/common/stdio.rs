//! The real `mct-mcp-server` binary driven over stdio (JSON-RPC, one message
//! per line), shared by the black-box tests that need a separate process.
//!
//! A test pulls it in with
//! `#[path = "common/stdio.rs"] mod stdio;`. Test-specific helpers go in the
//! test's own `impl StdioServer` block, built on [`StdioServer::request`].

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

/// How many trailing stderr lines are kept to print when a test panics.
const STDERR_TAIL_LINES: usize = 64;

pub struct StdioServer {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
    diagnostics: Arc<Mutex<VecDeque<String>>>,
    stderr_reader: Option<std::thread::JoinHandle<()>>,
}

impl StdioServer {
    /// Spawns the server on `root` with `extra_args` and completes the MCP
    /// handshake, asserting `initialize` succeeded.
    pub fn start(root: &Path, extra_args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mct-mcp-server"))
            .arg("--root")
            .arg(root)
            .args(extra_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let diagnostics = Arc::new(Mutex::new(VecDeque::new()));
        let captured = diagnostics.clone();
        let stderr_reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut tail = captured.lock().unwrap();
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line.chars().take(2048).collect());
            }
        });
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
            diagnostics,
            stderr_reader: Some(stderr_reader),
        };
        let init = server.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "mct-stdio-test", "version": "0"}
            }),
        );
        assert!(init["error"].is_null(), "initialize failed: {init}");
        server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        server
    }

    pub fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Sends a request and returns the response carrying its id, skipping
    /// any notification the server emits in between.
    pub fn request(&mut self, method: &str, params: Value) -> Value {
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
}

impl Drop for StdioServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
        if std::thread::panicking() {
            eprintln!("isolated server stderr (last {STDERR_TAIL_LINES} lines):");
            // A second panic here would abort and lose these diagnostics.
            let tail = self.diagnostics.lock().unwrap_or_else(|e| e.into_inner());
            for line in tail.iter() {
                eprintln!("{line}");
            }
        }
    }
}
