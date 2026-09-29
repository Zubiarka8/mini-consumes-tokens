//! The query cache (issue #16) end to end, through the real tools: exact and
//! semantic hits, every validation that must turn a candidate into a miss,
//! granular invalidation, and the token saving of a hit. The semantic path
//! runs on a deterministic fake embedder whose query vectors are set per test,
//! so similarities are exact rather than whatever a real model makes of them.
//! Latency is measured separately, by `examples/cache_benchmark.rs`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use mct_index::{Embedder, ExcludeSet, Index};
use mct_mcp_server::cache::{CacheConfig, CacheStats, ResponseMode};
use mct_mcp_server::server::{HybridSearchArgs, MctServer, SearchSymbolsArgs, ServerOptions};
use rmcp::handler::server::wrapper::Parameters;

const DIM: usize = 16;

/// Symbol texts get a hashed bag of words; queries listed in `queries` (keyed
/// by their split, lowercased words) get exactly the vector given.
struct FakeEmbedder {
    model: String,
    queries: HashMap<String, Vec<f32>>,
}

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

impl Embedder for FakeEmbedder {
    fn model_id(&self) -> &str {
        &self.model
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Ok(texts.iter().map(|t| bag_of_words(t)).collect())
    }
    fn embed_query(&self, query: &str) -> Result<Vec<f32>, String> {
        let key = query.split_whitespace().collect::<Vec<_>>().join(" ");
        Ok(self
            .queries
            .get(&key)
            .cloned()
            .unwrap_or_else(|| bag_of_words(query)))
    }
}

fn unit(i: usize) -> Vec<f32> {
    let mut v = vec![0.0; DIM];
    v[i] = 1.0;
    v
}

/// `cos * a + sin * b` for orthonormal `a`, `b`: similarity `cos` to `a`.
fn at_similarity(a: usize, b: usize, cos: f32) -> Vec<f32> {
    let mut v = vec![0.0; DIM];
    v[a] = cos;
    v[b] = (1.0 - cos * cos).sqrt();
    v
}

/// The vectors every semantic test uses.
fn query_vectors() -> HashMap<String, Vec<f32>> {
    let mut q = HashMap::new();
    q.insert("create invoice".to_string(), unit(0));
    q.insert("make a new invoice".to_string(), at_similarity(0, 1, 0.99));
    q.insert("build invoice".to_string(), at_similarity(0, 1, 0.90));
    // Two cached queries 0.94 apart (too far for one to hit the other), and
    // a third 0.975 from one and 0.970 from the other: a best match above the
    // threshold with no clear margin.
    q.insert("send notification".to_string(), unit(2));
    q.insert("notify user".to_string(), at_similarity(2, 3, 0.94));
    let (x, y) = (
        0.975f32,
        (0.970 - 0.94 * 0.975) / (1.0 - 0.94f32 * 0.94).sqrt(),
    );
    let mut ambiguous = vec![0.0; DIM];
    ambiguous[2] = x;
    ambiguous[3] = y;
    ambiguous[4] = (1.0 - x * x - y * y).sqrt();
    q.insert("alert user".to_string(), ambiguous);
    q
}

/// A throwaway project the tests can edit.
fn project(tag: &str) -> PathBuf {
    // Unique per call: tests run in parallel and some share a tag.
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-query-cache-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/billing.rs"),
        "pub fn create_invoice(customer: &str) -> String {\n    format!(\"inv-{customer}\")\n}\n\n\
         pub fn cancel_invoice(id: &str) -> bool {\n    !id.is_empty()\n}\n",
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

fn options(cache: CacheConfig, model: &str) -> ServerOptions {
    ServerOptions {
        cache,
        embedder: Some(Box::new(FakeEmbedder {
            model: model.to_string(),
            queries: query_vectors(),
        })),
        shared_cache: None,
    }
}

fn server_with(root: &Path, options: ServerOptions) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::with_options(index, registry, options)
}

fn server(root: &Path) -> MctServer {
    server_with(root, options(CacheConfig::default(), "fake/m1"))
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

fn hybrid_args(query: &str) -> HybridSearchArgs {
    HybridSearchArgs {
        query: query.to_string(),
        // Explicit, so paraphrases share a key (auto routing may differ).
        alpha: Some(0.75),
        path: None,
        language: None,
        top_k: None,
        offset: None,
        snippet_lines: None,
        format: None,
        cache: None,
    }
}

async fn search(server: &MctServer, args: SearchSymbolsArgs) -> String {
    text(server.search_symbols(Parameters(args)).await.unwrap())
}

async fn hybrid(server: &MctServer, args: HybridSearchArgs) -> String {
    text(server.hybrid_search(Parameters(args)).await.unwrap())
}

fn is_reference(reply: &str) -> bool {
    reply.starts_with("cache: ") && !reply.contains('\n')
}

/// What `server`'s index answers with no cache at all, for comparison.
async fn uncached_search(root: &Path, args: SearchSymbolsArgs) -> String {
    search(
        &server_with(root, options(CacheConfig::disabled(), "fake/m1")),
        args,
    )
    .await
}

async fn uncached_hybrid(root: &Path, args: HybridSearchArgs) -> String {
    hybrid(
        &server_with(root, options(CacheConfig::disabled(), "fake/m1")),
        args,
    )
    .await
}

// ---- exact path -------------------------------------------------------

#[tokio::test]
async fn an_identical_repeat_is_an_exact_hit_answered_by_reference() {
    let root = project("exact-hit");
    let server = server(&root);
    let first = search(&server, search_args("invoice")).await;
    assert!(first.contains("create_invoice"), "got: {first}");
    assert_eq!(first, uncached_search(&root, search_args("invoice")).await);

    let second = search(&server, search_args("invoice")).await;
    assert_eq!(
        second,
        "cache: unchanged since your identical earlier call (cache:false resends)"
    );
    // Whitespace-only differences normalise to the same key.
    let spaced = search(&server, search_args("  invoice ")).await;
    assert!(is_reference(&spaced), "got: {spaced}");

    let stats = server.cache_stats();
    assert_eq!((stats.exact_hit, stats.exact_miss), (2, 1), "{stats:?}");
    assert_eq!(stats.reference_responses, 2);
}

#[tokio::test]
async fn cache_false_resends_the_full_result() {
    let root = project("bypass");
    let server = server(&root);
    let first = search(&server, search_args("invoice")).await;
    let resent = search(
        &server,
        SearchSymbolsArgs {
            cache: Some(false),
            ..search_args("invoice")
        },
    )
    .await;
    assert_eq!(resent, first);
    assert_eq!(server.cache_stats().exact_hit, 0);
}

#[tokio::test]
async fn a_different_query_or_ranking_parameter_misses() {
    let root = project("params");
    let server = server(&root);
    search(&server, search_args("invoice")).await;
    for args in [
        search_args("invoices"),
        SearchSymbolsArgs {
            path: Some("src/billing.rs".to_string()),
            ..search_args("invoice")
        },
        SearchSymbolsArgs {
            language: Some("python".to_string()),
            ..search_args("invoice")
        },
    ] {
        let reply = search(&server, args).await;
        assert!(!is_reference(&reply), "got: {reply}");
    }
    let stats = server.cache_stats();
    assert_eq!((stats.exact_hit, stats.exact_miss), (0, 4), "{stats:?}");
}

#[tokio::test]
async fn presentation_parameters_reuse_the_ranking_but_render_afresh() {
    let root = project("presentation");
    let server = server(&root);
    search(&server, search_args("invoice")).await;
    let paged = SearchSymbolsArgs {
        offset: Some(1),
        snippet_lines: Some(2),
        format: Some("toon".to_string()),
        ..search_args("invoice")
    };
    let reply = search(&server, paged).await;
    // A hit (same ranking), but this page was never sent: full text, and
    // exactly what an uncached server renders.
    assert_eq!(server.cache_stats().exact_hit, 1);
    let paged = SearchSymbolsArgs {
        offset: Some(1),
        snippet_lines: Some(2),
        format: Some("toon".to_string()),
        ..search_args("invoice")
    };
    assert_eq!(reply, uncached_search(&root, paged).await);
}

#[tokio::test]
async fn a_different_tool_or_alpha_never_shares_an_entry() {
    let root = project("tools");
    let server = server(&root);
    search(&server, search_args("invoice")).await;
    let lexical_hybrid = hybrid(
        &server,
        HybridSearchArgs {
            alpha: Some(0.0),
            ..hybrid_args("invoice")
        },
    )
    .await;
    assert!(
        lexical_hybrid.starts_with("hybrid: alpha 0"),
        "got: {lexical_hybrid}"
    );
    let other_alpha = hybrid(
        &server,
        HybridSearchArgs {
            alpha: Some(0.5),
            ..hybrid_args("invoice")
        },
    )
    .await;
    assert!(!is_reference(&other_alpha), "got: {other_alpha}");
    let stats = server.cache_stats();
    assert_eq!((stats.exact_hit, stats.exact_miss), (0, 3), "{stats:?}");
}

// ---- semantic path ----------------------------------------------------

#[tokio::test]
async fn an_equivalent_query_is_a_validated_semantic_hit() {
    let root = project("semantic-hit");
    let server = server(&root);
    let original = hybrid(&server, hybrid_args("create invoice")).await;
    assert!(original.contains("create_invoice"), "got: {original}");

    let paraphrase = hybrid(&server, hybrid_args("make a new invoice")).await;
    assert_eq!(
        paraphrase,
        "cache: same as your earlier `create invoice` (sim 0.990); cache:false resends"
    );
    let stats = server.cache_stats();
    assert_eq!(
        (
            stats.semantic_candidate,
            stats.semantic_hit,
            stats.semantic_miss
        ),
        (1, 1, 1),
        "{stats:?}"
    );
    assert_eq!(stats.embedding_latency.count, 2);
}

#[tokio::test]
async fn a_semantic_hit_in_full_mode_says_whose_ranking_it_reuses() {
    let root = project("semantic-full");
    let config = CacheConfig {
        response: ResponseMode::Full,
        ..CacheConfig::default()
    };
    let server = server_with(&root, options(config, "fake/m1"));
    let original = hybrid(&server, hybrid_args("create invoice")).await;
    let paraphrase = hybrid(&server, hybrid_args("make a new invoice")).await;
    assert!(
        paraphrase.ends_with(
            "cache: ranking reused from earlier query `create invoice` (similarity 0.990), re-validated against the current index\n"
        ),
        "got: {paraphrase}"
    );
    // Same ranked symbols as the source query, rendered for this call.
    let body = |t: &str| {
        t.lines()
            .filter(|l| l.starts_with("src/"))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(body(&paraphrase), body(&original));
    // An exact repeat in full mode is the full text again, unannotated.
    assert_eq!(
        hybrid(&server, hybrid_args("create invoice")).await,
        original
    );
}

#[tokio::test]
async fn a_similarity_below_the_threshold_is_not_even_a_candidate() {
    let root = project("semantic-below");
    let server = server(&root);
    hybrid(&server, hybrid_args("create invoice")).await;
    let reply = hybrid(&server, hybrid_args("build invoice")).await;
    assert!(reply.starts_with("hybrid: alpha 0.75"), "got: {reply}");
    let stats = server.cache_stats();
    assert_eq!(
        (stats.semantic_candidate, stats.semantic_hit),
        (0, 0),
        "{stats:?}"
    );
    assert_eq!(
        reply,
        uncached_hybrid(&root, hybrid_args("build invoice")).await
    );
}

#[tokio::test]
async fn a_close_runner_up_makes_the_match_ambiguous_and_a_miss() {
    let root = project("semantic-ambiguous");
    let server = server(&root);
    hybrid(&server, hybrid_args("send notification")).await;
    hybrid(&server, hybrid_args("notify user")).await;
    let reply = hybrid(&server, hybrid_args("alert user")).await;
    assert!(!is_reference(&reply), "got: {reply}");
    assert!(!reply.contains("cache:"), "got: {reply}");
    let stats = server.cache_stats();
    assert_eq!(
        (
            stats.semantic_candidate,
            stats.ambiguous_match,
            stats.semantic_hit
        ),
        (1, 1, 0),
        "{stats:?}"
    );
    assert_eq!(
        reply,
        uncached_hybrid(&root, hybrid_args("alert user")).await
    );
}

#[tokio::test]
async fn another_embedding_model_shares_nothing() {
    let root = project("model-change");
    // Same repository and cache, different model: neither the exact nor the
    // semantic path may reuse a ranking (or compare vectors) across models.
    for query in ["create invoice", "make a new invoice"] {
        let first = server(&root);
        hybrid(&first, hybrid_args("create invoice")).await;
        let mut switched = options(CacheConfig::default(), "fake/m2");
        switched.shared_cache = Some(first.cache_handle());
        let second = server_with(&root, switched);
        let reply = hybrid(&second, hybrid_args(query)).await;
        assert!(reply.contains("(fake/m2)"), "got: {reply}");
        assert!(!reply.contains("cache:"), "got: {reply}");
        let stats = second.cache_stats();
        assert_eq!(
            (stats.exact_hit, stats.exact_miss, stats.semantic_candidate),
            (0, 2, 0),
            "{query}: {stats:?}"
        );
    }
}

#[tokio::test]
async fn disabling_the_semantic_cache_keeps_the_exact_one() {
    let root = project("semantic-off");
    let config = CacheConfig {
        semantic: false,
        ..CacheConfig::default()
    };
    let server = server_with(&root, options(config, "fake/m1"));
    hybrid(&server, hybrid_args("create invoice")).await;
    let paraphrase = hybrid(&server, hybrid_args("make a new invoice")).await;
    assert!(!paraphrase.contains("cache:"), "got: {paraphrase}");
    let repeat = hybrid(&server, hybrid_args("create invoice")).await;
    assert!(is_reference(&repeat), "got: {repeat}");
    let stats = server.cache_stats();
    assert_eq!(
        (stats.semantic_candidate, stats.exact_hit),
        (0, 1),
        "{stats:?}"
    );
    assert_eq!(stats.embedding_latency.count, 0);
}

#[tokio::test]
async fn a_disabled_cache_changes_nothing() {
    let root = project("disabled");
    let server = server_with(&root, options(CacheConfig::disabled(), "fake/m1"));
    let first = hybrid(&server, hybrid_args("create invoice")).await;
    assert_eq!(hybrid(&server, hybrid_args("create invoice")).await, first);
    assert_eq!(
        search(&server, search_args("invoice")).await,
        search(&server, search_args("invoice")).await
    );
    assert_eq!(server.cache_stats(), CacheStats::default());
}

// ---- validation and invalidation --------------------------------------

#[tokio::test]
async fn deleting_a_ranked_symbol_is_a_stale_rejection() {
    let root = project("stale");
    let server = server(&root);
    search(&server, search_args("invoice")).await;
    std::fs::write(
        root.join("src/billing.rs"),
        "pub fn cancel_invoice(id: &str) -> bool {\n    !id.is_empty()\n}\n",
    )
    .unwrap();
    reindex(&server).await;
    let reply = search(&server, search_args("invoice")).await;
    assert!(!reply.contains("create_invoice"), "got: {reply}");
    assert_eq!(reply, uncached_search(&root, search_args("invoice")).await);
    let stats = server.cache_stats();
    assert_eq!(
        (stats.stale_rejection, stats.exact_hit),
        (1, 0),
        "{stats:?}"
    );
}

#[tokio::test]
async fn editing_a_referenced_file_is_a_fingerprint_mismatch() {
    let root = project("fingerprint");
    let server = server(&root);
    let before = search(&server, search_args("invoice")).await;
    // Every ranked symbol keeps its row, but its file's content changed.
    let path = root.join("src/billing.rs");
    let source = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{source}// trailing comment\n")).unwrap();
    reindex(&server).await;
    let reply = search(&server, search_args("invoice")).await;
    assert_eq!(reply, before, "same ranking, rendered in full again");
    let stats = server.cache_stats();
    assert_eq!(
        (
            stats.fingerprint_mismatch,
            stats.stale_rejection,
            stats.exact_hit
        ),
        (1, 0, 0),
        "{stats:?}"
    );
}

#[tokio::test]
async fn an_unrelated_change_keeps_the_entry_after_revalidation() {
    let root = project("unrelated");
    let server = server(&root);
    // In a repo this small every symbol is one of hybrid_search's semantic
    // candidates — every file is referenced by an unscoped result — so the
    // hybrid call is scoped to keep `src/other.rs` out of its result.
    let scoped = || HybridSearchArgs {
        path: Some("src/billing.rs".to_string()),
        ..hybrid_args("create invoice")
    };
    search(&server, search_args("invoice")).await;
    hybrid(&server, scoped()).await;
    // Same symbols, new body: the file's hash (and the repository state)
    // moves, no ranked symbol's file does.
    std::fs::write(
        root.join("src/other.rs"),
        "pub fn unrelated_helper() -> u32 {\n    43\n}\n",
    )
    .unwrap();
    reindex(&server).await;

    let lexical = search(&server, search_args("invoice")).await;
    assert!(is_reference(&lexical), "got: {lexical}");
    let semantic = hybrid(&server, scoped()).await;
    assert!(is_reference(&semantic), "got: {semantic}");
    let stats = server.cache_stats();
    assert_eq!(
        (
            stats.exact_hit,
            stats.repository_mismatch,
            stats.fingerprint_mismatch
        ),
        (2, 0, 0),
        "{stats:?}"
    );
}

#[tokio::test]
async fn a_change_elsewhere_that_alters_the_ranking_is_a_repository_mismatch() {
    let root = project("ranking-change");
    let server = server(&root);
    search(&server, search_args("invoice")).await;
    // No referenced file changed, but a new symbol now matches the query.
    std::fs::write(
        root.join("src/other.rs"),
        "pub fn unrelated_helper() -> u32 {\n    42\n}\n\npub fn invoice_total() -> u32 {\n    0\n}\n",
    )
    .unwrap();
    reindex(&server).await;
    let reply = search(&server, search_args("invoice")).await;
    assert!(reply.contains("invoice_total"), "got: {reply}");
    assert_eq!(reply, uncached_search(&root, search_args("invoice")).await);
    let stats = server.cache_stats();
    assert_eq!(
        (stats.repository_mismatch, stats.exact_hit),
        (1, 0),
        "{stats:?}"
    );
}

#[tokio::test]
async fn a_hit_never_shows_stale_source() {
    let root = project("fresh-source");
    let server = server(&root);
    let with_snippet = || SearchSymbolsArgs {
        snippet_lines: Some(3),
        ..search_args("create_invoice")
    };
    let before = search(&server, with_snippet()).await;
    assert!(
        before.contains("format!(\"inv-{customer}\")"),
        "got: {before}"
    );
    // The file changes on disk but the index hasn't caught up yet: the
    // ranking is still valid (a hit), the source shown must be today's.
    let path = root.join("src/billing.rs");
    let source = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, source.replace("inv-", "INV-")).unwrap();
    let after = search(&server, with_snippet()).await;
    assert_eq!(server.cache_stats().exact_hit, 1);
    assert!(
        after.contains("format!(\"INV-{customer}\")"),
        "got: {after}"
    );
    assert!(!after.contains("\"inv-"), "got: {after}");
}

#[tokio::test]
async fn a_hit_on_the_semantic_path_is_revalidated_too() {
    let root = project("semantic-stale");
    let server = server(&root);
    hybrid(&server, hybrid_args("create invoice")).await;
    std::fs::write(
        root.join("src/billing.rs"),
        "pub fn cancel_invoice(id: &str) -> bool {\n    !id.is_empty()\n}\n",
    )
    .unwrap();
    reindex(&server).await;
    let reply = hybrid(&server, hybrid_args("make a new invoice")).await;
    assert!(!reply.contains("cache:"), "got: {reply}");
    let stats = server.cache_stats();
    assert_eq!(
        (
            stats.semantic_candidate,
            stats.stale_rejection,
            stats.semantic_hit
        ),
        (1, 1, 0),
        "{stats:?}"
    );
}

#[tokio::test]
async fn the_status_report_shows_the_cache_counters_once_used() {
    let root = project("status");
    let server = server(&root);
    let status = |s: MctServer| async move {
        text(
            s.get_indexing_status(Parameters(Default::default()))
                .await
                .unwrap(),
        )
    };
    assert!(!status(server.clone()).await.contains("Query cache"));
    search(&server, search_args("invoice")).await;
    search(&server, search_args("invoice")).await;
    let report = status(server.clone()).await;
    assert!(
        report.contains("Query cache (semantic on, threshold 0.95"),
        "got: {report}"
    );
    assert!(report.contains("exact: 1 hit, 1 miss"), "got: {report}");
}

#[tokio::test]
async fn the_entry_limit_holds() {
    let root = project("eviction");
    let server = server_with(
        &root,
        options(
            CacheConfig {
                max_entries: 2,
                ..CacheConfig::default()
            },
            "fake/m1",
        ),
    );
    for q in ["invoice", "cancel", "notification"] {
        search(&server, search_args(q)).await;
    }
    assert_eq!(server.cache_stats().entries, 2);
    // "invoice" was the least recently used: evicted, so a miss.
    let reply = search(&server, search_args("invoice")).await;
    assert!(!is_reference(&reply), "got: {reply}");
}

// ---- token reduction --------------------------------------------------

fn approx_tokens(text: &str) -> f64 {
    text.chars().count() as f64 / 4.0
}

fn reduction(uncached: &str, cached: &str) -> f64 {
    1.0 - approx_tokens(cached) / approx_tokens(uncached)
}

/// A project whose default search page is full (10 of many hits), the shape
/// of a real repository's answer.
fn large_project() -> PathBuf {
    let dir = project("tokens");
    let mut source = String::new();
    for i in 0..40 {
        source.push_str(&format!(
            "pub fn create_invoice_line_{i}(amount: u32) -> u32 {{\n    amount + {i}\n}}\n\n"
        ));
    }
    std::fs::write(dir.join("src/invoice_lines.rs"), source).unwrap();
    dir
}

#[tokio::test]
async fn exact_hits_cut_tokens_by_at_least_ninety_percent() {
    let root = large_project();
    let server = server(&root);
    let uncached = search(&server, search_args("create invoice")).await;
    let cached = search(&server, search_args("create invoice")).await;
    assert!(is_reference(&cached), "got: {cached}");
    let r = reduction(&uncached, &cached);
    assert!(r >= 0.90, "exact: {r:.3} ({uncached:?} vs {cached:?})");
}

#[tokio::test]
async fn semantic_hits_cut_tokens_by_at_least_ninety_percent() {
    let root = large_project();
    let server = server(&root);
    hybrid(&server, hybrid_args("create invoice")).await;
    // The paraphrase's own uncached answer is what a hit replaces.
    let uncached = uncached_hybrid(&root, hybrid_args("make a new invoice")).await;
    let cached = hybrid(&server, hybrid_args("make a new invoice")).await;
    assert!(is_reference(&cached), "got: {cached}");
    let r = reduction(&uncached, &cached);
    assert!(r >= 0.90, "semantic: {r:.3} ({uncached:?} vs {cached:?})");
}
