//! ERR-022: the server answers `initialize` and `tools/list` while the
//! startup indexing is still running, and no tool serves an index that is not
//! complete. The state is driven by hand through `Readiness`, so the tests do
//! not depend on how long indexing takes.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::time::Duration;

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::readiness::Readiness;
use mct_mcp_server::server::MctServer;
use rmcp::ServiceExt;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

/// A server around an index that was opened but never indexed, as at startup.
fn unindexed_server(ready_wait: Duration) -> MctServer {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compute-app");
    let registry = mct_mcp_server::registry::build_registry();
    let index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    MctServer::new(index, registry).with_ready_wait(ready_wait)
}

/// What the background startup task does.
async fn index_now(server: &MctServer) {
    let registry = mct_mcp_server::registry::build_registry();
    let handle = server.index_handle();
    let mut index = handle.lock().await;
    index.reindex(&registry, false).unwrap();
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
        Self {
            next_id: 1,
            reader: BufReader::new(read).lines(),
            writer,
        }
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

    async fn initialize(&mut self) -> Value {
        let reply = self
            .request(
                "initialize",
                json!({"protocolVersion":"2024-11-05","capabilities":{},
                       "clientInfo":{"name":"readiness-test","version":"1"}}),
            )
            .await;
        let note = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        self.writer
            .write_all(format!("{note}\n").as_bytes())
            .await
            .unwrap();
        reply
    }

    async fn tool(&mut self, name: &str, arguments: Value) -> (bool, String) {
        let reply = self
            .request("tools/call", json!({"name": name, "arguments": arguments}))
            .await;
        let result = &reply["result"];
        let text = result["content"][0]["text"].as_str().unwrap_or_default();
        (
            result["isError"].as_bool().unwrap_or(false),
            text.to_string(),
        )
    }
}

#[tokio::test]
async fn initialize_and_tools_list_do_not_wait_for_the_index() {
    let server = unindexed_server(Duration::from_millis(50));
    server.readiness().begin_indexing();
    // Hold the index lock as the startup indexing does.
    let handle = server.index_handle();
    let _indexing = handle.lock().await;

    let mut client = Client::connect(server).await;
    let init = client.initialize().await;
    assert!(init["result"]["capabilities"]["tools"].is_object());
    let listed = client.request("tools/list", json!({})).await;
    assert!(listed["result"]["tools"].as_array().unwrap().len() > 10);
}

#[tokio::test]
async fn tools_refuse_while_indexing_and_answer_once_ready() {
    let server = unindexed_server(Duration::from_millis(50));
    let readiness = server.readiness();
    readiness.begin_indexing();
    let mut client = Client::connect(server.clone()).await;
    client.initialize().await;

    // The startup indexing holds the index lock for its whole run.
    let handle = server.index_handle();
    let indexing = handle.lock().await;

    let (is_error, text) = client.tool("find_symbol", json!({"name": "main"})).await;
    assert!(is_error, "a tool must not answer from a half-built index");
    assert!(text.contains("index_state: indexing"), "{text}");

    // These must answer without waiting for that lock.
    let (is_error, text) = client.tool("get_indexing_status", json!({})).await;
    assert!(!is_error);
    assert!(text.contains("index_state: indexing"), "{text}");
    let (is_error, text) = client.tool("discover_tool_categories", json!({})).await;
    assert!(!is_error, "{text}");

    drop(indexing);
    index_now(&server).await;
    readiness.mark_ready();
    let (is_error, text) = client.tool("find_symbol", json!({"name": "main"})).await;
    assert!(!is_error, "{text}");
    assert!(!text.contains("index_state"), "{text}");
}

#[tokio::test]
async fn a_tool_call_waits_for_indexing_that_finishes_in_time() {
    let server = unindexed_server(Duration::from_secs(30));
    let readiness = server.readiness();
    readiness.begin_indexing();
    let mut client = Client::connect(server.clone()).await;
    client.initialize().await;

    let finisher = {
        let server = server.clone();
        tokio::spawn(async move {
            index_now(&server).await;
            readiness.mark_ready();
        })
    };
    let (is_error, text) = client.tool("find_symbol", json!({"name": "main"})).await;
    finisher.await.unwrap();
    assert!(!is_error, "{text}");
}

#[tokio::test]
async fn a_failed_startup_is_reported_and_reindex_recovers_it() {
    let server = unindexed_server(Duration::from_millis(50));
    server.readiness().begin_indexing();
    server.readiness().mark_failed("disk full");
    let mut client = Client::connect(server.clone()).await;
    client.initialize().await;

    let (is_error, text) = client.tool("find_symbol", json!({"name": "main"})).await;
    assert!(is_error);
    assert!(
        text.contains("index_state: failed") && text.contains("disk full"),
        "{text}"
    );
    let (_, status) = client.tool("get_indexing_status", json!({})).await;
    assert!(status.contains("disk full"), "{status}");

    let (is_error, text) = client.tool("reindex", json!({"force": false})).await;
    assert!(
        !is_error,
        "reindex must be allowed after a failed startup: {text}"
    );
    let (is_error, text) = client.tool("find_symbol", json!({"name": "main"})).await;
    assert!(!is_error, "{text}");
}

#[tokio::test]
async fn a_server_around_an_indexed_index_is_ready_without_any_marking() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compute-app");
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let server = MctServer::new(index, registry);
    assert_eq!(
        server.readiness().state(),
        Readiness::ready().state(),
        "existing callers must keep working unchanged"
    );
    let mut client = Client::connect(server).await;
    client.initialize().await;
    let (is_error, text) = client.tool("find_symbol", json!({"name": "main"})).await;
    assert!(!is_error, "{text}");
}

#[tokio::test]
async fn a_batch_of_index_free_queries_answers_while_indexing() {
    let server = unindexed_server(Duration::from_millis(50));
    server.readiness().begin_indexing();
    let mut client = Client::connect(server.clone()).await;
    client.initialize().await;
    let handle = server.index_handle();
    let _indexing = handle.lock().await;

    let (is_error, text) = client
        .tool(
            "batch",
            json!({"queries": [{"tool": "get_indexing_status"}, {"tool": "discover_tool_categories"}]}),
        )
        .await;
    assert!(!is_error, "{text}");
    assert!(text.contains("index_state: indexing"), "{text}");
    // The second query is the tool catalogue, not a refusal.
    assert!(text.contains("find_symbol"), "{text}");
    assert_eq!(
        text.matches("[2] discover_tool_categories\nindex_state")
            .count(),
        0,
        "{text}"
    );
}

#[tokio::test]
async fn a_batch_waits_for_indexing_once_not_once_per_query() {
    let server = unindexed_server(Duration::from_millis(500));
    server.readiness().begin_indexing();
    let mut client = Client::connect(server.clone()).await;
    client.initialize().await;
    let handle = server.index_handle();
    let _indexing = handle.lock().await;

    let query = json!({"tool": "find_symbol", "args": {"name": "main"}});
    let started = std::time::Instant::now();
    let (is_error, text) = client
        .tool(
            "batch",
            json!({"queries": [{"tool": "get_indexing_status"}, query, query, query]}),
        )
        .await;
    let elapsed = started.elapsed();
    assert!(!is_error, "{text}");
    // The status report mentions the rule too: one status plus three refusals.
    assert_eq!(text.matches("No partial results").count(), 4, "{text}");
    assert!(
        elapsed < Duration::from_millis(1100),
        "three index-dependent queries waited {elapsed:?} instead of one wait of 500ms"
    );
}

#[tokio::test]
async fn an_index_free_query_does_not_cut_short_the_wait_of_the_next_one() {
    // The indexing ends 150ms in, inside the 400ms wait: a query after
    // `get_indexing_status` must wait for it exactly as a direct call would.
    let server = unindexed_server(Duration::from_millis(400));
    let readiness = server.readiness();
    readiness.begin_indexing();
    let mut client = Client::connect(server.clone()).await;
    client.initialize().await;
    let finisher = {
        let server = server.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            index_now(&server).await;
            readiness.mark_ready();
        })
    };
    let (is_error, text) = client
        .tool(
            "batch",
            json!({"queries": [{"tool": "get_indexing_status"},
                               {"tool": "find_symbol", "args": {"name": "main"}}]}),
        )
        .await;
    finisher.await.unwrap();
    assert!(!is_error, "{text}");
    let answer = text.split("[2] find_symbol").nth(1).unwrap_or_default();
    assert!(!answer.contains("index_state"), "{text}");
}
