//! Failed tool calls land in the project's incident log
//! (`<root>/.mct-index/errors/`) while the client receives exactly the answer
//! it would receive without the log.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{BatchArgs, BatchQuery, MctServer};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::ServiceExt;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

#[path = "common/content.rs"]
mod content;
use content::content_of;

#[path = "common/tempdir.rs"]
mod tempdir;
use tempdir::tempdir;

/// An indexed project that already has `.mct-index/`, like one after `init`.
fn project() -> PathBuf {
    let root = tempdir("mct-mcp-incidents-");
    std::fs::write(root.join("lib.rs"), "pub fn alpha() {}\n").unwrap();
    std::fs::create_dir(root.join(".mct-index")).unwrap();
    root
}

fn server_for(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

fn errors_dir(root: &Path) -> PathBuf {
    root.join(".mct-index").join("errors")
}

fn records(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(errors_dir(root)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn read(root: &Path, name: &str) -> String {
    std::fs::read_to_string(errors_dir(root).join(name)).unwrap()
}

fn query(tool: &str, args: Value) -> BatchQuery {
    BatchQuery {
        tool: tool.to_string(),
        args: args.as_object().cloned(),
    }
}

async fn run_batch(server: &MctServer, queries: Vec<BatchQuery>) -> String {
    let result = server
        .batch(Parameters(BatchArgs { queries }))
        .await
        .unwrap();
    content_of(&result)
}

fn mixed_queries() -> Vec<BatchQuery> {
    vec![
        query("find_symbol", json!({"name": "alpha"})),
        // A wrong type: serde's message would echo the value sent.
        query("find_symbol", json!({"name": "alpha", "limit": "hunter2"})),
        query("no_such_tool", json!({})),
        query("reindex", json!({})),
    ]
}

#[tokio::test]
async fn failed_sub_queries_are_recorded_while_the_batch_still_answers() {
    let root = project();
    let server = server_for(&root);
    let text = run_batch(&server, mixed_queries()).await;

    assert!(text.contains("3 failed"), "{text}");
    assert!(
        text.contains("alpha"),
        "the good sub-query still answers: {text}"
    );
    // Three failures, three records, each its own file.
    let names = records(&root);
    assert_eq!(names.len(), 3, "{names:?}");
    assert!(
        names
            .iter()
            .all(|n| n.contains("--batch--invalid-params--")),
        "{names:?}"
    );
    let bodies: Vec<String> = names.iter().map(|n| read(&root, n)).collect();
    assert!(
        bodies.iter().any(|b| b.contains("find_symbol:")),
        "{bodies:?}"
    );
    assert!(
        bodies.iter().any(|b| b.contains("no_such_tool:")),
        "{bodies:?}"
    );
    assert!(bodies.iter().any(|b| b.contains("reindex:")), "{bodies:?}");
    // The value the caller sent is not kept.
    assert!(bodies.iter().all(|b| !b.contains("hunter2")), "{bodies:?}");
}

#[tokio::test]
async fn a_blocked_log_directory_leaves_the_batch_answer_unchanged() {
    let usable = project();
    let blocked = project();
    std::fs::write(errors_dir(&blocked), "occupied").unwrap();

    let with_log = run_batch(&server_for(&usable), mixed_queries()).await;
    let without_log = run_batch(&server_for(&blocked), mixed_queries()).await;

    // Same text apart from nothing at all: the log adds no byte to the reply.
    assert_eq!(with_log, without_log);
    assert_eq!(
        std::fs::read_to_string(errors_dir(&blocked)).unwrap(),
        "occupied"
    );
    assert_eq!(records(&usable).len(), 3);
}

#[tokio::test]
async fn a_batch_without_failures_records_nothing() {
    let root = project();
    let server = server_for(&root);
    let text = run_batch(
        &server,
        vec![query("find_symbol", json!({"name": "alpha"}))],
    )
    .await;
    assert!(!text.contains("failed"), "{text}");
    assert!(records(&root).is_empty());
    assert!(!errors_dir(&root).exists());
}

#[tokio::test]
async fn a_project_without_an_index_directory_gets_no_directory() {
    let root = project();
    std::fs::remove_dir(root.join(".mct-index")).unwrap();
    let server = server_for(&root);
    let text = run_batch(&server, mixed_queries()).await;
    assert!(text.contains("3 failed"), "{text}");
    assert!(!root.join(".mct-index").exists());
}

struct Client {
    next_id: u64,
    reader: tokio::io::Lines<BufReader<ReadHalf<DuplexStream>>>,
    writer: WriteHalf<DuplexStream>,
}

impl Client {
    async fn connect(server: MctServer) -> Self {
        let (server_io, client_io) = tokio::io::duplex(1 << 20);
        tokio::spawn(async move {
            if let Ok(running) = server.serve(server_io).await {
                let _ = running.waiting().await;
            }
        });
        let (read, writer) = tokio::io::split(client_io);
        let mut client = Self {
            next_id: 1,
            reader: BufReader::new(read).lines(),
            writer,
        };
        client
            .request(
                "initialize",
                json!({"protocolVersion":"2024-11-05","capabilities":{},
                       "clientInfo":{"name":"incident-test","version":"1"}}),
            )
            .await;
        let note = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        client
            .writer
            .write_all(format!("{note}\n").as_bytes())
            .await
            .unwrap();
        client
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let line = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        self.writer
            .write_all(format!("{line}\n").as_bytes())
            .await
            .unwrap();
        let reply = tokio::time::timeout(Duration::from_secs(20), self.reader.next_line())
            .await
            .expect("the server must reply, not hang")
            .unwrap()
            .expect("the server closed the stream");
        serde_json::from_str(&reply).unwrap()
    }

    async fn tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
            .await
    }
}

#[tokio::test]
async fn an_invalid_direct_call_is_recorded_and_answered_as_before() {
    let usable = project();
    let blocked = project();
    std::fs::write(errors_dir(&blocked), "occupied").unwrap();
    let args = json!({"name": "alpha", "limit": "hunter2"});

    let mut client = Client::connect(server_for(&usable)).await;
    let with_log = client.tool("find_symbol", args.clone()).await;
    let mut client = Client::connect(server_for(&blocked)).await;
    let without_log = client.tool("find_symbol", args).await;

    // rmcp refuses the arguments with an error result; it is the same either way.
    assert_eq!(with_log["result"]["isError"], json!(true), "{with_log}");
    assert_eq!(with_log["result"], without_log["result"]);

    let names = records(&usable);
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(
        names[0].contains("--tools-call--invalid-params--"),
        "{}",
        names[0]
    );
    let body = read(&usable, &names[0]);
    assert!(body.contains("find_symbol:"), "{body}");
    assert!(!body.contains("hunter2"), "{body}");
}

#[tokio::test]
async fn a_successful_direct_call_records_nothing() {
    let root = project();
    let mut client = Client::connect(server_for(&root)).await;
    let reply = client.tool("find_symbol", json!({"name": "alpha"})).await;
    assert!(reply["error"].is_null(), "{reply}");
    assert!(records(&root).is_empty());
}

#[tokio::test]
async fn a_refusal_while_the_index_is_not_ready_is_recorded() {
    let root = project();
    let registry = mct_mcp_server::registry::build_registry();
    let index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let server = MctServer::new(index, registry).with_ready_wait(Duration::from_millis(50));
    server.readiness().begin_indexing();
    let handle = server.index_handle();
    let _indexing = handle.lock().await;
    let mut client = Client::connect(server).await;

    let reply = client.tool("find_symbol", json!({"name": "alpha"})).await;
    assert_eq!(reply["result"]["isError"], json!(true), "{reply}");
    let names = records(&root);
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(
        names[0].contains("--tools-call--not-ready--"),
        "{}",
        names[0]
    );
    assert!(read(&root, &names[0]).contains("index_state: indexing"));
}
