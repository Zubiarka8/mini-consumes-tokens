//! A path that the project's current ignore rules exclude is never served,
//! even while the index still holds rows from before the rule was written:
//! an edit to `.mctignore` takes effect for the very next tool call, not only
//! after the watcher's (debounced, possibly disabled) or a manual reindex.
//!
//! The fixture is synthetic: a legitimate control file under `src/`, and a
//! `secret_area/` whose every file carries the marker `sentinel` (a symbol, a
//! Markdown literal, a dependency name) or is only identifiable by its path
//! (a file that fails to parse). Every read-only tool is asked about it after
//! the rule is written; none may answer with the marker or the path, beyond
//! echoing the caller's own arguments.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use mct_index::{ExcludeSet, Index, IGNORE_FILE_NAME};
use mct_mcp_server::server::{BatchArgs, BatchQuery, MctServer};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use serde_json::{json, Value};

/// Lower-cased, the only words that would reveal the excluded directory.
const LEAK_MARKERS: [&str; 2] = ["sentinel", "secret_area"];

fn tempdir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-exclusion-hygiene-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// `src/public_api.py` (the control), and `secret_area/` whose function
/// calls the control's `helper_control` — so even a query about a public
/// symbol (its callers) can reach into the excluded directory.
fn write_fixture(dir: &Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("secret_area")).unwrap();
    fs::write(
        dir.join("src/public_api.py"),
        "def public_control_function():\n    return helper_control()\n\n\n\
         def helper_control():\n    return 1\n",
    )
    .unwrap();
    fs::write(
        dir.join("secret_area/notes.py"),
        "def mct_private_sentinel_7f3a():\n    return helper_control()\n\n\n\
         def sentinel_7f3a_value():\n    return \"MCT_PRIVATE_SENTINEL_7F3A\"\n",
    )
    .unwrap();
    fs::write(
        dir.join("secret_area/notes.md"),
        // Prose, not a token-like string: the literal filter drops suspected
        // secrets, and this paragraph must be indexed for the test to bite.
        "# Sentinel 7f3a heading\n\n\
         This private paragraph holds the sentinel phrase for literal search.\n",
    )
    .unwrap();
    fs::write(dir.join("secret_area/broken.py"), "def (\n").unwrap();
    fs::write(
        dir.join("secret_area/package.json"),
        r#"{"name": "fixture", "dependencies": {"sentinel-7f3a-dep": "1.0.0"}}"#,
    )
    .unwrap();
}

fn exclude_secret_area(dir: &Path) {
    fs::write(dir.join(IGNORE_FILE_NAME), "secret_area/\n").unwrap();
}

/// Every string in `args`, at any depth: what the caller already knows.
fn arg_strings(args: &Value, out: &mut Vec<String>) {
    match args {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|v| arg_strings(v, out)),
        Value::Object(map) => map.values().for_each(|v| arg_strings(v, out)),
        _ => {}
    }
}

/// The marker `text` reveals, once the caller's own arguments echoed back
/// (`No definition of `<name>``) are removed — echoing input leaks nothing.
fn leaked_marker(text: &str, args: &Value) -> Option<&'static str> {
    let mut echoes = Vec::new();
    arg_strings(args, &mut echoes);
    // Longest first, so a quoted phrase goes before its unquoted words.
    echoes.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let mut scrubbed = text.to_string();
    for echo in echoes.iter().filter(|s| !s.is_empty()) {
        scrubbed = scrubbed.replace(echo.as_str(), "");
    }
    let scrubbed = scrubbed.to_lowercase();
    LEAK_MARKERS
        .into_iter()
        .find(|marker| scrubbed.contains(marker))
}

/// Every read-only tool, aimed at the excluded content directly, through a
/// public symbol whose callers include it, or project-wide.
fn surfaces() -> Vec<(&'static str, Value)> {
    vec![
        ("list_symbols", json!({"path": "secret_area"})),
        ("list_symbols", json!({"path": "secret_area/notes.py"})),
        ("find_symbol", json!({"name": "mct_private_sentinel_7f3a"})),
        ("find_symbol", json!({"name": "sentinel", "match": "fuzzy"})),
        ("search_symbols", json!({"query": "sentinel"})),
        ("hybrid_search", json!({"query": "sentinel"})),
        (
            "hybrid_search",
            json!({"query": "\"private paragraph holds\""}),
        ),
        (
            "semantic_search",
            json!({"query": "private sentinel value"}),
        ),
        ("find_references", json!({"symbol": "helper_control"})),
        ("find_callers", json!({"function": "helper_control"})),
        (
            "find_calls",
            json!({"function": "mct_private_sentinel_7f3a"}),
        ),
        ("impact_analysis", json!({"symbol": "helper_control"})),
        ("build_context_pack", json!({"symbol": "helper_control"})),
        (
            "build_context_pack",
            json!({"symbol": "mct_private_sentinel_7f3a"}),
        ),
        ("get_file_skeleton", json!({"path": "secret_area/notes.py"})),
        ("get_project_overview", json!({})),
        ("get_file_tree", json!({})),
        ("get_indexing_status", json!({"verbose_dependencies": true})),
        ("find_dead_code", json!({})),
    ]
}

fn content_of(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A tool's text, or its error message — an error must not leak either.
async fn call(server: &MctServer, tool: &str, args: &Value) -> String {
    let args = args.as_object().cloned();
    match server.call_read_only_tool(tool, args).await {
        Ok(result) => content_of(&result),
        Err(err) => err.message.to_string(),
    }
}

async fn run_batch(server: &MctServer, queries: &[(&str, Value)]) -> String {
    let queries = queries
        .iter()
        .map(|(tool, args)| BatchQuery {
            tool: tool.to_string(),
            args: args.as_object().cloned(),
        })
        .collect();
    content_of(
        &server
            .batch(Parameters(BatchArgs { queries }))
            .await
            .unwrap(),
    )
}

/// An indexed fixture behind a server sharing the project's own rules, as
/// `main` builds it — but with no watcher, so nothing reindexes on its own.
fn server_for(dir: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(dir, ExcludeSet::for_project(dir)).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

#[tokio::test]
async fn a_newly_excluded_path_is_not_served_before_any_reindex() {
    let dir = tempdir();
    write_fixture(&dir);
    let server = server_for(&dir);

    // Preconditions: before the rule, the excluded content is reachable
    // through each kind of row (symbol, relation, literal, issue,
    // dependency), and the lexical query below is cached.
    let before = [
        ("find_symbol", json!({"name": "mct_private_sentinel_7f3a"})),
        ("find_callers", json!({"function": "helper_control"})),
        (
            "hybrid_search",
            json!({"query": "\"private paragraph holds\""}),
        ),
        ("get_indexing_status", json!({"verbose_dependencies": true})),
        ("search_symbols", json!({"query": "sentinel"})),
    ];
    for (tool, args) in &before {
        let text = call(&server, tool, args).await;
        assert!(
            leaked_marker(&text, args).is_some(),
            "precondition: {tool} {args} should reach the fixture before the rule:\n{text}"
        );
    }
    let status = call(&server, "get_indexing_status", &json!({})).await;
    assert!(status.contains("secret_area/broken.py"), "{status}");
    assert!(status.contains("1 manifests"), "{status}");

    exclude_secret_area(&dir);
    // Reproduce the original stale-row defect before the MCP boundary sweeps.
    assert!(!server
        .index_handle()
        .lock()
        .await
        .find_symbol("mct_private_sentinel_7f3a")
        .unwrap()
        .is_empty());

    for (tool, args) in surfaces() {
        let text = call(&server, tool, &args).await;
        assert_eq!(
            leaked_marker(&text, &args),
            None,
            "{tool} {args} served excluded content:\n{text}"
        );
        assert!(
            !text.contains("same as your earlier"),
            "{tool} {args} vouched for a cached pre-exclusion answer:\n{text}"
        );
    }
    let batch_queries = [
        ("find_symbol", json!({"name": "mct_private_sentinel_7f3a"})),
        ("find_callers", json!({"function": "helper_control"})),
        ("get_file_tree", json!({})),
    ];
    let batch = run_batch(&server, &batch_queries).await;
    let batch_args = Value::Array(batch_queries.iter().map(|(_, a)| a.clone()).collect());
    assert_eq!(leaked_marker(&batch, &batch_args), None, "batch:\n{batch}");

    // The control file is still indexed and served.
    let control = call(
        &server,
        "find_symbol",
        &json!({"name": "public_control_function"}),
    )
    .await;
    assert!(control.contains("src/public_api.py"), "{control}");
    let callers = call(
        &server,
        "find_callers",
        &json!({"function": "helper_control"}),
    )
    .await;
    assert!(callers.contains("public_control_function"), "{callers}");
}

#[tokio::test]
async fn removing_the_rule_serves_the_path_again_without_a_reindex() {
    let dir = tempdir();
    write_fixture(&dir);
    exclude_secret_area(&dir);
    let server = server_for(&dir);
    let args = json!({"name": "mct_private_sentinel_7f3a"});
    let hidden = call(&server, "find_symbol", &args).await;
    assert_eq!(leaked_marker(&hidden, &args), None, "{hidden}");

    fs::write(dir.join(IGNORE_FILE_NAME), "# nothing excluded\n").unwrap();
    let shown = call(&server, "find_symbol", &args).await;
    assert!(shown.contains("secret_area/notes.py"), "{shown}");
}

/// The real server binary over stdio — the `tools/call` path every MCP
/// client takes — with the watcher disabled, so only the server's own
/// per-call check can apply a rule written while it runs.
struct StdioServer {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
}

impl StdioServer {
    fn start(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_mct-mcp-server"))
            .arg("--root")
            .arg(root)
            .args(["--reindex-debounce-ms", "0"])
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
        server.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "exclusion-privacy-test", "version": "0"}
            }),
        );
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

    /// A tool's text content, or its JSON-RPC error message.
    fn call(&mut self, tool: &str, args: &Value) -> String {
        let response = self.request("tools/call", json!({"name": tool, "arguments": args}));
        if let Some(message) = response["error"]["message"].as_str() {
            return message.to_string();
        }
        response["result"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Drop for StdioServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn the_stdio_server_applies_a_rule_written_while_it_runs() {
    let dir = tempdir();
    write_fixture(&dir);
    let mut server = StdioServer::start(&dir);
    let private = json!({"name": "mct_private_sentinel_7f3a"});
    let before = server.call("find_symbol", &private);
    assert!(before.contains("secret_area/notes.py"), "{before}");

    exclude_secret_area(&dir);

    let queries = [
        ("find_symbol", private.clone()),
        ("find_callers", json!({"function": "helper_control"})),
        ("get_file_skeleton", json!({"path": "secret_area/notes.py"})),
        ("get_file_tree", json!({})),
        ("get_indexing_status", json!({"verbose_dependencies": true})),
        (
            "batch",
            json!({"queries": [{"tool": "find_symbol", "args": {"name": "mct_private_sentinel_7f3a"}}]}),
        ),
    ];
    for (tool, args) in &queries {
        let text = server.call(tool, args);
        assert_eq!(
            leaked_marker(&text, args),
            None,
            "{tool} {args} served excluded content:\n{text}"
        );
    }
    let control = server.call("find_symbol", &json!({"name": "public_control_function"}));
    assert!(control.contains("src/public_api.py"), "{control}");
}

#[tokio::test]
async fn an_unreadable_rule_file_does_not_reexpose_private_content() {
    for imported in [false, true] {
        let dir = tempdir();
        write_fixture(&dir);
        let rules_file = if imported {
            fs::write(dir.join(IGNORE_FILE_NAME), "@import-gitignore\n").unwrap();
            dir.join(".gitignore")
        } else {
            dir.join(IGNORE_FILE_NAME)
        };
        fs::write(&rules_file, "secret_area/\n").unwrap();
        let server = server_for(&dir);
        let args = json!({"name": "mct_private_sentinel_7f3a"});
        let hidden = call(&server, "find_symbol", &args).await;
        assert_eq!(leaked_marker(&hidden, &args), None, "{hidden}");

        // Invalid UTF-8 is a portable read failure, not removal of the rule.
        fs::write(&rules_file, [0xff]).unwrap();
        let result = server
            .call_read_only_tool("find_symbol", args.as_object().cloned())
            .await;
        assert!(
            result.is_err(),
            "a rule read failure must reject the query: {result:?}"
        );
        let retry = server
            .call_read_only_tool("find_symbol", args.as_object().cloned())
            .await;
        assert!(
            retry.is_err(),
            "an unresolved read failure must keep rejecting queries"
        );

        fs::write(&rules_file, "secret_area/\n").unwrap();
        let hidden = call(&server, "find_symbol", &args).await;
        assert_eq!(leaked_marker(&hidden, &args), None, "{hidden}");
        let control = call(
            &server,
            "find_symbol",
            &json!({"name": "public_control_function"}),
        )
        .await;
        assert!(control.contains("src/public_api.py"), "{control}");
    }
}

#[tokio::test]
async fn an_imported_gitignore_edit_is_applied_before_the_next_query() {
    let dir = tempdir();
    write_fixture(&dir);
    fs::write(dir.join(IGNORE_FILE_NAME), "@import-gitignore\n").unwrap();
    fs::write(dir.join(".gitignore"), "# nothing excluded\n").unwrap();
    let server = server_for(&dir);
    let args = json!({"name": "mct_private_sentinel_7f3a"});
    assert!(call(&server, "find_symbol", &args)
        .await
        .contains("secret_area/notes.py"));
    fs::write(dir.join(".gitignore"), "secret_area/\n").unwrap();
    let hidden = call(&server, "find_symbol", &args).await;
    assert_eq!(leaked_marker(&hidden, &args), None, "{hidden}");
    let control = call(
        &server,
        "find_symbol",
        &json!({"name": "public_control_function"}),
    )
    .await;
    assert!(control.contains("src/public_api.py"), "{control}");
}

#[cfg(unix)]
#[tokio::test]
async fn unreadable_parent_does_not_retain_excluded_symbols_issues_or_dependencies() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir();
    write_fixture(&dir);
    let locked = dir.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::rename(dir.join("secret_area"), locked.join("secret_area")).unwrap();
    let server = server_for(&dir);
    let status = call(
        &server,
        "get_indexing_status",
        &json!({"verbose_dependencies": true}),
    )
    .await;
    assert!(status.contains("sentinel-7f3a-dep"), "{status}");
    assert!(status.contains("secret_area/broken.py"), "{status}");
    fs::write(dir.join(IGNORE_FILE_NAME), "locked/secret_area/\n").unwrap();

    let previous = fs::metadata(&locked).unwrap().permissions();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o0)).unwrap();
    if fs::read_dir(&locked).is_ok() {
        fs::set_permissions(&locked, previous).unwrap();
        eprintln!("permission-denial fixture unavailable to this privileged user");
        return;
    }
    let args = json!({"name": "mct_private_sentinel_7f3a"});
    let hidden = call(&server, "find_symbol", &args).await;
    fs::set_permissions(&locked, previous).unwrap();
    assert_eq!(leaked_marker(&hidden, &args), None, "{hidden}");
    let status = call(
        &server,
        "get_indexing_status",
        &json!({"verbose_dependencies": true}),
    )
    .await;
    assert_eq!(leaked_marker(&status, &json!({})), None, "{status}");
    let control = call(
        &server,
        "find_symbol",
        &json!({"name": "public_control_function"}),
    )
    .await;
    assert!(control.contains("src/public_api.py"), "{control}");
}
