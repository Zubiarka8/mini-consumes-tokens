//! R03 semantic-runtime audit: `hybrid_search` must not hold the index mutex
//! while it waits on the embedding model, ordinary queries must stay
//! responsive meanwhile, and a reindex that lands during that wait must not
//! be ranked from the stale view or leave vectors for deleted symbols.
//!
//! The regressions use a deterministic embedder whose query embedding blocks
//! on a gate the test controls (no sleeps, no timing assertions: a regression
//! shows as the embedder's own bounded wait expiring). The `semantic`-feature
//! test at the bottom runs the real local model and prints the timings that
//! `benchmarks/semantic-runtime.md` records; it is `#[ignore]`d because the
//! first run downloads the model and embeds a whole repository.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use mct_index::{Embedder, ExcludeSet, Index};
use mct_mcp_server::cache::CacheConfig;
use mct_mcp_server::server::{HybridSearchArgs, MctServer, SearchSymbolsArgs, ServerOptions};
use rmcp::handler::server::wrapper::Parameters;
use tokio::sync::Notify;

const DIM: usize = 16;
const MODEL: &str = "fake/gate";

/// How long a blocked embedder waits for the test before giving up and
/// recording that something held it up. Generous: it only matters on failure.
const GATE_LIMIT: Duration = Duration::from_secs(10);

fn bag_of_words(text: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; DIM];
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let h = word.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
        });
        v[(h % DIM as u64) as usize] += 1.0;
    }
    v
}

/// A latch the test opens; waiting on it is bounded.
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }

    /// Whether the gate was opened before `limit` ran out.
    fn wait_open(&self, limit: Duration) -> bool {
        let guard = self.open.lock().unwrap();
        let (guard, _) = self
            .changed
            .wait_timeout_while(guard, limit, |open| !*open)
            .unwrap();
        *guard
    }
}

/// Symbol texts embed instantly; the *query* embedding signals `entered`, then
/// blocks until the gate opens — standing in for ONNX inference.
struct BlockingEmbedder {
    entered: Arc<Notify>,
    gate: Arc<Gate>,
    timed_out: Arc<AtomicBool>,
    query_calls: Arc<AtomicUsize>,
    /// When set, symbol embedding (the pending-symbol refresh) waits on this
    /// gate instead, and signals the paired notify when it starts waiting.
    symbol_gate: Option<(Arc<Gate>, Arc<Notify>)>,
}

impl Embedder for BlockingEmbedder {
    fn model_id(&self) -> &str {
        MODEL
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if let Some((gate, entered)) = &self.symbol_gate {
            entered.notify_one();
            if !gate.wait_open(GATE_LIMIT) {
                self.timed_out.store(true, Ordering::SeqCst);
            }
        }
        Ok(texts.iter().map(|t| bag_of_words(t)).collect())
    }
    fn embed_query(&self, query: &str) -> Result<Vec<f32>, String> {
        self.query_calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        if !self.gate.wait_open(GATE_LIMIT) {
            self.timed_out.store(true, Ordering::SeqCst);
        }
        Ok(bag_of_words(query))
    }
}

/// A throwaway project the tests can edit.
fn project(tag: &str) -> PathBuf {
    // Unique per call: tests run in parallel and some share a tag.
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-semantic-audit-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/billing.rs"),
        "pub fn create_invoice(customer: &str) -> String {\n    format!(\"inv-{customer}\")\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/notify.rs"),
        "pub fn send_notification(user: &str) -> usize {\n    user.len()\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/other.rs"),
        "pub fn unrelated_helper() -> u32 {\n    42\n}\n",
    )
    .unwrap();
    dir.canonicalize().unwrap()
}

fn server_with(root: &Path, options: ServerOptions) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::with_options(index, registry, options)
}

async fn reindex(server: &MctServer) {
    let registry = server.registry_handle();
    server
        .index_handle()
        .lock()
        .await
        .reindex(&registry, false)
        .unwrap();
}

fn text(result: rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

fn search_args(query: &str) -> SearchSymbolsArgs {
    SearchSymbolsArgs {
        query: query.to_string(),
        path: None,
        language: None,
        limit: None,
        offset: None,
        snippet_lines: None,
        format: None,
        cache: None,
    }
}

fn hybrid_args(query: &str, alpha: f64) -> HybridSearchArgs {
    HybridSearchArgs {
        query: query.to_string(),
        alpha: Some(alpha),
        path: None,
        language: None,
        top_k: None,
        offset: None,
        snippet_lines: None,
        format: None,
        cache: None,
    }
}

async fn search(server: &MctServer, query: &str) -> String {
    text(
        server
            .search_symbols(Parameters(search_args(query)))
            .await
            .unwrap(),
    )
}

async fn hybrid(server: &MctServer, query: &str, alpha: f64) -> String {
    text(
        server
            .hybrid_search(Parameters(hybrid_args(query, alpha)))
            .await
            .unwrap(),
    )
}

struct Blocked {
    server: MctServer,
    root: PathBuf,
    entered: Arc<Notify>,
    gate: Arc<Gate>,
    timed_out: Arc<AtomicBool>,
    query_calls: Arc<AtomicUsize>,
}

fn blocked_server(
    tag: &str,
    cache: CacheConfig,
    symbol_gate: Option<(Arc<Gate>, Arc<Notify>)>,
) -> Blocked {
    let entered = Arc::new(Notify::new());
    let gate = Arc::new(Gate::default());
    let timed_out = Arc::new(AtomicBool::new(false));
    let query_calls = Arc::new(AtomicUsize::new(0));
    let root = project(tag);
    let server = server_with(
        &root,
        ServerOptions {
            cache,
            embedder: Some(Box::new(BlockingEmbedder {
                entered: Arc::clone(&entered),
                gate: Arc::clone(&gate),
                timed_out: Arc::clone(&timed_out),
                query_calls: Arc::clone(&query_calls),
                symbol_gate,
            })),
            shared_cache: None,
        },
    );
    Blocked {
        server,
        root,
        entered,
        gate,
        timed_out,
        query_calls,
    }
}

/// While one `hybrid_search` waits on the model for its query vector, lexical
/// queries, an alpha-0 `hybrid_search` and a full reindex all complete — the
/// index mutex is not held — and the blocked call then ranks the *new* index:
/// the deleted file's symbol is gone from its answer and from the embedded
/// set (no vector resurrected), and every live symbol has a vector.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_index_mutex_is_free_while_the_query_embedding_blocks() {
    let b = blocked_server("gate", CacheConfig::default(), None);
    let semantic = {
        let server = b.server.clone();
        tokio::spawn(async move { hybrid(&server, "send notification", 0.75).await })
    };
    b.entered.notified().await;

    let lexical = search(&b.server, "create_invoice").await;
    assert!(lexical.contains("create_invoice"), "{lexical}");
    let alpha_zero = hybrid(&b.server, "unrelated_helper", 0.0).await;
    assert!(alpha_zero.contains("unrelated_helper"), "{alpha_zero}");

    std::fs::remove_file(b.root.join("src/notify.rs")).unwrap();
    reindex(&b.server).await;
    assert!(
        !b.timed_out.load(Ordering::SeqCst),
        "ordinary queries or the reindex waited for the model: the index mutex was held"
    );

    b.gate.open();
    let reply = semantic.await.unwrap();
    assert!(
        !reply.contains("send_notification"),
        "ranked from the pre-reindex view:\n{reply}"
    );
    assert!(reply.contains("create_invoice"), "{reply}");

    let index = b.server.index_handle();
    let index = index.lock().await;
    let coverage = index.embedding_coverage(MODEL).unwrap();
    assert!(coverage.total > 0);
    assert_eq!(
        coverage.embedded, coverage.total,
        "every live symbol, and nothing else, has a vector"
    );
    assert!(index.find_symbol("send_notification").unwrap().is_empty());
}

/// Concurrent semantic queries neither deadlock nor disagree: the model is
/// built once and shared, the index and cache locks are always taken in the
/// same order, and with the cache off every call takes the full path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_semantic_queries_agree_and_do_not_deadlock() {
    let b = blocked_server("concurrent", CacheConfig::disabled(), None);
    b.gate.open();
    let (a, b2, c, d) = tokio::join!(
        hybrid(&b.server, "create invoice", 0.75),
        hybrid(&b.server, "create invoice", 0.75),
        hybrid(&b.server, "create invoice", 0.75),
        search(&b.server, "create_invoice"),
    );
    assert_eq!(a, b2);
    assert_eq!(a, c);
    assert!(d.contains("create_invoice"), "{d}");
    assert!(a.contains("create_invoice"), "{a}");
    assert_eq!(b.query_calls.load(Ordering::SeqCst), 3);
    assert!(!b.timed_out.load(Ordering::SeqCst));
}

/// A quoted phrase and alpha 0 never touch the model: the query embedder is
/// not called, so neither can wait on it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lexical_only_queries_never_reach_the_model() {
    let b = blocked_server("lexical", CacheConfig::default(), None);
    let phrase = hybrid(&b.server, "\"create invoice\"", 0.75).await;
    assert!(phrase.starts_with("hybrid:"), "{phrase}");
    let _ = hybrid(&b.server, "create_invoice", 0.0).await;
    assert_eq!(b.query_calls.load(Ordering::SeqCst), 0);
}

/// The pending-symbol refresh holds the index mutex only for its snapshot and
/// each commit: while a semantic query waits on the symbol embedder, lexical
/// queries and a reindex complete, and the blocked call then ranks the new
/// index with no vector left for the deleted symbol.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_index_mutex_is_free_while_pending_symbols_embed() {
    let symbol_gate = Arc::new(Gate::default());
    let symbols_entered = Arc::new(Notify::new());
    let b = blocked_server(
        "symbols",
        CacheConfig::disabled(),
        Some((Arc::clone(&symbol_gate), Arc::clone(&symbols_entered))),
    );
    // The query itself is not gated: only the symbol embedding waits.
    b.gate.open();
    let semantic = {
        let server = b.server.clone();
        tokio::spawn(async move { hybrid(&server, "send notification", 0.75).await })
    };
    symbols_entered.notified().await;

    let lexical = search(&b.server, "create_invoice").await;
    assert!(lexical.contains("create_invoice"), "{lexical}");

    std::fs::remove_file(b.root.join("src/notify.rs")).unwrap();
    reindex(&b.server).await;
    assert!(
        !b.timed_out.load(Ordering::SeqCst),
        "ordinary queries or the reindex waited for the symbol embedder: the index mutex was held"
    );

    symbol_gate.open();
    let reply = semantic.await.unwrap();
    assert!(
        !reply.contains("send_notification"),
        "ranked from the pre-reindex view:\n{reply}"
    );
    assert!(reply.contains("create_invoice"), "{reply}");

    let index = b.server.index_handle();
    let index = index.lock().await;
    let coverage = index.embedding_coverage(MODEL).unwrap();
    assert!(coverage.total > 0);
    assert_eq!(
        coverage.embedded, coverage.total,
        "every live symbol, and nothing else, has a vector"
    );
}

/// Real local model (`bge-small-en-v1.5` via fastembed): cold start, warm
/// queries, and ordinary-query latency while the cold `hybrid_search` runs.
/// Prints a table for `benchmarks/semantic-runtime.md`.
///
/// `cargo test --locked -p mct-mcp-server --features semantic --test semantic_audit \
///    -- --ignored --nocapture real_model`
#[cfg(feature = "semantic")]
#[ignore = "not a bug: downloads the model on first use and embeds this whole repository"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_model_cold_warm_and_concurrent_latency() {
    use std::time::Instant;

    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    // A freshly indexed server, so every cold start embeds all symbols.
    let fresh = || {
        let registry = mct_mcp_server::registry::build_registry();
        let mut index = Index::open_in_memory(&repo, ExcludeSet::default()).unwrap();
        let t = Instant::now();
        index.reindex(&registry, false).unwrap();
        eprintln!("index: {:?}", t.elapsed());
        MctServer::with_options(
            index,
            registry,
            ServerOptions {
                cache: CacheConfig::disabled(),
                embedder: None,
                shared_cache: None,
            },
        )
    };

    // Cold start with nothing else queued behind it: the wall time of one
    // first query, load plus embedding plus ranking.
    let quiet = fresh();
    let t = Instant::now();
    let reply = hybrid(&quiet, "load settings from disk", 0.75).await;
    eprintln!("cold hybrid_search, quiet: {:?}", t.elapsed());
    eprintln!("{}", reply.lines().next().unwrap_or_default());
    assert!(
        reply.contains("fastembed/bge-small-en-v1.5"),
        "the real model did not run: {}",
        reply.lines().next().unwrap_or_default()
    );
    drop(quiet);

    // The same cold start with an ordinary query looping alongside it: how
    // long any other tool waits while the model works.
    let server = fresh();
    let done = Arc::new(AtomicBool::new(false));
    let cold_started = Instant::now();
    let cold = {
        let (server, done) = (server.clone(), Arc::clone(&done));
        tokio::spawn(async move {
            let reply = hybrid(&server, "load settings from disk", 0.75).await;
            done.store(true, Ordering::SeqCst);
            (reply, cold_started.elapsed())
        })
    };
    let mut ordinary: Vec<(Duration, Duration)> = Vec::new();
    while !done.load(Ordering::SeqCst) {
        let at = cold_started.elapsed();
        let t = Instant::now();
        let _ = search(&server, "refresh_embeddings").await;
        ordinary.push((at, t.elapsed()));
    }
    let (reply, cold_total) = cold.await.unwrap();
    eprintln!("cold hybrid_search, with ordinary probe: {cold_total:?}");
    eprintln!("{}", reply.lines().next().unwrap_or_default());
    assert!(
        reply.contains("fastembed/bge-small-en-v1.5"),
        "the real model did not run: {}",
        reply.lines().next().unwrap_or_default()
    );
    let worst = ordinary.iter().map(|(_, l)| *l).max().unwrap_or_default();
    eprintln!(
        "ordinary search_symbols during the cold call: {} samples, worst {worst:?}",
        ordinary.len()
    );
    for (at, latency) in &ordinary {
        eprintln!("  at +{at:?}: {latency:?}");
    }

    let mut warm = Vec::new();
    for q in [
        "parse source into symbols",
        "walk directory tree",
        "rank search results",
        "write index to disk",
        "split identifier words",
    ] {
        let t = Instant::now();
        let _ = hybrid(&server, q, 0.75).await;
        warm.push(t.elapsed());
    }
    warm.sort();
    eprintln!("warm hybrid_search: median {:?}, all {warm:?}", warm[2]);

    // Eight queries at once on the warm server: wall time of the burst, and
    // each query's own latency (they overlap on the blocking pool).
    let burst = [
        "parse source into symbols",
        "walk directory tree",
        "rank search results",
        "write index to disk",
        "split identifier words",
        "load settings from disk",
        "refresh embeddings for changed files",
        "format tool output",
    ];
    let started = Instant::now();
    let mut set = tokio::task::JoinSet::new();
    for q in burst {
        let server = server.clone();
        set.spawn(async move {
            let t = Instant::now();
            let _ = hybrid(&server, q, 0.75).await;
            t.elapsed()
        });
    }
    let mut each = Vec::new();
    while let Some(latency) = set.join_next().await {
        each.push(latency.unwrap());
    }
    let wall = started.elapsed();
    each.sort();
    eprintln!(
        "8 parallel hybrid_search: wall {wall:?}, per query min {:?} max {:?}",
        each[0],
        each[each.len() - 1]
    );
}
