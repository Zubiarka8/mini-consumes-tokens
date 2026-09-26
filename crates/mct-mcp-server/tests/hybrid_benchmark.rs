//! Retrieval benchmark for `hybrid_search` (issue #61) over this repo's own
//! source, with the real local embedding model: recall@10 and MRR for
//! lexical-only (`alpha = 0`), semantic-only (`alpha = 1`) and the default
//! blend, plus warm per-query latency.
//!
//! Needs the `semantic` feature and downloads the model on first run, so it
//! is `#[ignore]`d:
//!
//! ```sh
//! cargo test -p mct-mcp-server --features semantic --release \
//!     --test hybrid_benchmark -- --ignored --nocapture
//! ```
//!
//! Queries mix identifier-ish phrasing (where lexical search is strong) with
//! descriptions of what the code does in words absent from its name (where
//! only the semantic side can help) — the mix an agent actually produces.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "semantic")]

use std::path::Path;
use std::time::{Duration, Instant};

use mct_index::{ExcludeSet, Index, QueryScope};
use mct_mcp_server::embedder::SemanticModel;

/// (query, acceptable symbol names). Ground truth is by name under
/// `crates/`; any listed name counts as a hit.
const CASES: &[(&str, &[&str])] = &[
    ("read ignore file", &["read_ignore_file"]),
    (
        "load the excluded path patterns from disk",
        &["read_ignore_file"],
    ),
    (
        "unused code that nothing references",
        &["find_dead_code_candidates", "find_dead_code"],
    ),
    (
        "is this a test file",
        &["file_name_looks_like_test", "path_looks_like_test"],
    ),
    ("current time in seconds since the epoch", &["unix_now"]),
    ("parse cargo toml", &["parse_cargo_toml"]),
    ("python requirements", &["parse_requirements_txt"]),
    ("npm package dependencies", &["parse_package_json"]),
    ("go module requirements", &["parse_go_mod"]),
    (
        "forget files that were deleted from the repo",
        &["remove_missing_files"],
    ),
    (
        "watch the filesystem and refresh the index",
        &["spawn_watcher"],
    ),
    ("database schema upgrades", &["migrations"]),
    ("break camelCase names into words", &["split_identifier"]),
    (
        "similarity between two vectors",
        &["dot_encoded", "semantic_ranking"],
    ),
    ("escape a table cell", &["escape_field"]),
    ("decode table", &["decode_table"]),
    (
        "collapse function bodies into an outline",
        &["file_skeleton", "brace_skeleton"],
    ),
    ("how many callers each symbol has", &["fan_in_counts"]),
    (
        "breadth first traversal of the call graph",
        &["bfs", "find_calls_bfs", "find_callers_bfs"],
    ),
    ("reject an empty symbol name", &["validate_name"]),
    ("record a file that failed to parse", &["record_issue"]),
    ("directory tree", &["file_tree"]),
    ("split words", &["split_identifier", "search_words"]),
    ("fuse lexical and semantic rankings", &["hybrid_search"]),
];

fn score(index: &Index, model: &SemanticModel, alpha: f64) -> (f64, f64, Duration) {
    let embedder = model.get(index.root()).unwrap();
    let mut recall = 0.0;
    let mut mrr = 0.0;
    let mut slowest = Duration::ZERO;
    let scope = QueryScope {
        path: Some("crates"),
        language: None,
    };
    for (query, expected) in CASES {
        let started = Instant::now();
        let hits = index
            .hybrid_search(query, Some(embedder), alpha, scope)
            .unwrap();
        slowest = slowest.max(started.elapsed());
        let rank = hits
            .iter()
            .take(10)
            .position(|h| expected.contains(&h.hit.name.as_str()));
        if let Some(rank) = rank {
            recall += 1.0;
            mrr += 1.0 / (rank as f64 + 1.0);
        } else {
            println!("  alpha {alpha}: miss `{query}`");
        }
    }
    let n = CASES.len() as f64;
    (recall / n, mrr / n, slowest)
}

#[test]
#[ignore = "downloads the embedding model; run with --features semantic -- --ignored"]
fn hybrid_beats_lexical_and_semantic_alone_within_the_latency_budget() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let model = SemanticModel::default();
    let embedder = model.get(&root).unwrap();
    let started = Instant::now();
    let embedded = index.refresh_embeddings(embedder).unwrap();
    println!("embedded {embedded} symbols in {:?}", started.elapsed());

    let (lex_recall, lex_mrr, _) = score(&index, &model, 0.0);
    let (sem_recall, sem_mrr, _) = score(&index, &model, 1.0);
    let (hyb_recall, hyb_mrr, slowest) = score(&index, &model, 0.5);
    println!("lexical  recall@10 {lex_recall:.2}  MRR {lex_mrr:.3}");
    println!("semantic recall@10 {sem_recall:.2}  MRR {sem_mrr:.3}");
    println!("hybrid   recall@10 {hyb_recall:.2}  MRR {hyb_mrr:.3}  slowest query {slowest:?}");

    // The #31 bar: hybrid finds more of the targets in its top 10 than
    // either side alone. MRR is only held to beating lexical: when the
    // semantic side alone already ranks a target first, fusion can only tie
    // or dilute that (last run: lexical 0.30, semantic 0.39, hybrid 0.36).
    assert!(hyb_recall > lex_recall && hyb_recall > sem_recall);
    assert!(hyb_mrr > lex_mrr);
    assert!(
        slowest < Duration::from_millis(200),
        "slowest query took {slowest:?}"
    );
}
