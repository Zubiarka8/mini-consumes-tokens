//! Retrieval benchmark for `hybrid_search` (issues #61, #63) over this
//! repo's own source, with the real local embedding model: recall@10 and MRR
//! per query bucket for lexical-only (`alpha = 0`), semantic-only
//! (`alpha = 1`) and the default blend, plus warm per-query latency.
//!
//! Needs the `semantic` feature and downloads the model on first run, so it
//! is `#[ignore]`d:
//!
//! ```sh
//! cargo test -p mct-mcp-server --features semantic --release \
//!     --test hybrid_benchmark -- --ignored --nocapture
//! ```
//!
//! Three buckets, the mix an agent actually produces: exact/partial
//! identifiers (where lexical search is strong), plain-language descriptions
//! of what the code does in words absent from its name (where only the
//! semantic side can help), and hierarchy-qualified names (`Type::method`,
//! `module::function`) that must land on one specific definition.
//!
//! [`PHRASE_CASES`] separately pins `"double-quoted"` exact-phrase queries
//! (literal names, error-message-like sentences, special characters) to the
//! top 1. Exact phrases never touch the model, so that test isn't ignored.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "semantic")]

use std::path::Path;
use std::time::{Duration, Instant};

use mct_index::{classify_query, ExcludeSet, HybridHit, Index, QueryScope};
use mct_mcp_server::embedder::SemanticModel;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bucket {
    Exact,
    Natural,
    Hierarchy,
}

const BUCKETS: [Bucket; 3] = [Bucket::Exact, Bucket::Natural, Bucket::Hierarchy];

/// (bucket, query, acceptable targets). A target is a symbol name under
/// `crates/`, or `name@fragment` to also require `fragment` in its path —
/// for names defined in several places (`new`, `parse`, `hybrid_search`).
/// Any listed target in the top 10 counts as a hit.
const CASES: &[(Bucket, &str, &[&str])] = &[
    // --- Exact / partial identifier -------------------------------------
    (Bucket::Exact, "read ignore file", &["read_ignore_file"]),
    (Bucket::Exact, "parse cargo toml", &["parse_cargo_toml"]),
    (Bucket::Exact, "decode table", &["decode_table"]),
    (
        Bucket::Exact,
        "split words",
        &["split_identifier", "search_words"],
    ),
    (
        Bucket::Exact,
        "find_dead_code_candidates",
        &["find_dead_code_candidates"],
    ),
    (Bucket::Exact, "fanInCounts", &["fan_in_counts"]),
    (Bucket::Exact, "unix_now", &["unix_now"]),
    (
        Bucket::Exact,
        "remove_missing_files",
        &["remove_missing_files"],
    ),
    (Bucket::Exact, "spawnWatcher", &["spawn_watcher"]),
    (Bucket::Exact, "validate_name", &["validate_name"]),
    (Bucket::Exact, "brace_skeleton", &["brace_skeleton"]),
    (Bucket::Exact, "LanguageRegistry", &["LanguageRegistry"]),
    (Bucket::Exact, "ExcludeSet", &["ExcludeSet"]),
    (Bucket::Exact, "SymbolRecord", &["SymbolRecord"]),
    (Bucket::Exact, "HybridHit", &["HybridHit"]),
    (Bucket::Exact, "embedding_text", &["embedding_text"]),
    (Bucket::Exact, "refresh embeddings", &["refresh_embeddings"]),
    (Bucket::Exact, "semantic ranking", &["semantic_ranking"]),
    (Bucket::Exact, "match_tier", &["match_tier"]),
    (
        Bucket::Exact,
        "fallback expression",
        &["fallback_expression"],
    ),
    (Bucket::Exact, "write parsed file", &["write_parsed_file"]),
    (Bucket::Exact, "parse_go_mod", &["parse_go_mod"]),
    (Bucket::Exact, "collect use names", &["collect_use_names"]),
    (Bucket::Exact, "receiver_type_name", &["receiver_type_name"]),
    (Bucket::Exact, "escape_field", &["escape_field"]),
    (Bucket::Exact, "parse csv line", &["parse_csv_line"]),
    (Bucket::Exact, "apply_ttc_catalog", &["apply_ttc_catalog"]),
    (Bucket::Exact, "page snippets", &["page_snippets"]),
    (Bucket::Exact, "impact analysis", &["impact_analysis"]),
    (Bucket::Exact, "get_file_skeleton", &["get_file_skeleton"]),
    (Bucket::Exact, "dotEncoded", &["dot_encoded"]),
    (Bucket::Exact, "mcp register", &["mcp_register"]),
    // Name known, verb swapped for a synonym.
    (Bucket::Exact, "fetch ignore file", &["read_ignore_file"]),
    (
        Bucket::Exact,
        "delete missing manifests",
        &["remove_missing_manifests"],
    ),
    (
        Bucket::Exact,
        "read indexing status",
        &["get_indexing_status"],
    ),
    // --- Natural-language intent ----------------------------------------
    (
        Bucket::Natural,
        "load the excluded path patterns from disk",
        &["read_ignore_file"],
    ),
    (
        Bucket::Natural,
        "unused code that nothing references",
        &["find_dead_code_candidates", "find_dead_code"],
    ),
    (
        Bucket::Natural,
        "is this a test file",
        &["file_name_looks_like_test", "path_looks_like_test"],
    ),
    (
        Bucket::Natural,
        "current time in seconds since the epoch",
        &["unix_now"],
    ),
    (
        Bucket::Natural,
        "python requirements",
        &["parse_requirements_txt"],
    ),
    (
        Bucket::Natural,
        "npm package dependencies",
        &["parse_package_json"],
    ),
    (Bucket::Natural, "go module requirements", &["parse_go_mod"]),
    (
        Bucket::Natural,
        "forget files that were deleted from the repo",
        &["remove_missing_files"],
    ),
    (
        Bucket::Natural,
        "watch the filesystem and refresh the index",
        &["spawn_watcher"],
    ),
    (Bucket::Natural, "database schema upgrades", &["migrations"]),
    (
        Bucket::Natural,
        "break camelCase names into words",
        &["split_identifier"],
    ),
    (
        Bucket::Natural,
        "similarity between two vectors",
        &["dot_encoded", "semantic_ranking"],
    ),
    (Bucket::Natural, "escape a table cell", &["escape_field"]),
    (
        Bucket::Natural,
        "collapse function bodies into an outline",
        &["file_skeleton", "brace_skeleton"],
    ),
    (
        Bucket::Natural,
        "how many callers each symbol has",
        &["fan_in_counts"],
    ),
    (
        Bucket::Natural,
        "breadth first traversal of the call graph",
        &["bfs", "find_calls_bfs", "find_callers_bfs"],
    ),
    (
        Bucket::Natural,
        "reject an empty symbol name",
        &["validate_name"],
    ),
    (
        Bucket::Natural,
        "record a file that failed to parse",
        &["record_issue"],
    ),
    (Bucket::Natural, "directory tree", &["file_tree"]),
    (
        Bucket::Natural,
        "fuse lexical and semantic rankings",
        &["hybrid_search"],
    ),
    (
        Bucket::Natural,
        "write the mcp json config for claude code",
        &["mcp_register"],
    ),
    (
        Bucket::Natural,
        "add the index folder to gitignore",
        &["gitignore_init"],
    ),
    (
        Bucket::Natural,
        "export unreferenced symbols as csv",
        &["dead_code_report", "csv_field"],
    ),
    (
        Bucket::Natural,
        "print coverage and health report",
        &["print_status"],
    ),
    (
        Bucket::Natural,
        "look up the parser for a file extension",
        &["for_extension"],
    ),
    (
        Bucket::Natural,
        "extract the name of the called function",
        &["call_target", "callee_identifier", "expr_name"],
    ),
    (
        Bucket::Natural,
        "go method receiver type",
        &["receiver_type_name"],
    ),
    (
        Bucket::Natural,
        "python import statements",
        &["import_names"],
    ),
    (
        Bucket::Natural,
        "first syntax error in the tree",
        &["first_error"],
    ),
    (
        Bucket::Natural,
        "convert a path to forward slashes relative to the root",
        &["to_relative_slash_path", "relative_slash_path"],
    ),
    (
        Bucket::Natural,
        "compact table output with one header row",
        &["encode_table"],
    ),
    (
        Bucket::Natural,
        "cap the response size and say it was truncated",
        &["truncation_note", "budget_note"],
    ),
    (
        Bucket::Natural,
        "list every tool grouped by category",
        &["tool_categories", "discover_tool_categories"],
    ),
    (
        Bucket::Natural,
        "normalize a vector to unit length",
        &["normalized"],
    ),
    (
        Bucket::Natural,
        "which directories should never be indexed",
        &[
            "DEFAULT_EXCLUDE_DIRS",
            "DEFAULT_EXCLUDE_PATTERNS",
            "is_excluded",
            "ExcludeSet",
        ],
    ),
    (
        Bucket::Natural,
        "rank matches with bm25 full text search",
        &["run_match", "search_symbols"],
    ),
    (
        Bucket::Natural,
        "download and load the embedding model",
        &["load@embedder.rs", "get@embedder.rs", "SemanticModel"],
    ),
    (
        Bucket::Natural,
        "wire up every language plugin",
        &["build_registry"],
    ),
    (
        Bucket::Natural,
        "turn a symbol kind enum into a string",
        &["symbol_kind_str"],
    ),
    (
        Bucket::Natural,
        "overview of the project with key symbols per module",
        &["get_project_overview", "overview"],
    ),
    // --- Module / struct hierarchy ---------------------------------------
    (
        Bucket::Hierarchy,
        "ExcludeSet::is_excluded",
        &["is_excluded"],
    ),
    (Bucket::Hierarchy, "ExcludeSet::new", &["new@exclude.rs"]),
    (
        Bucket::Hierarchy,
        "Index::hybrid_search",
        &["hybrid_search@mct-index/src/lib.rs"],
    ),
    (
        Bucket::Hierarchy,
        "Index::open_in_memory",
        &["open_in_memory"],
    ),
    (Bucket::Hierarchy, "Index.resolve_scope", &["resolve_scope"]),
    (
        Bucket::Hierarchy,
        "semantic::embedding_text",
        &["embedding_text"],
    ),
    (
        Bucket::Hierarchy,
        "search::split_identifier",
        &["split_identifier"],
    ),
    (Bucket::Hierarchy, "toon::decode_table", &["decode_table"]),
    (
        Bucket::Hierarchy,
        "format::impact_analysis",
        &["impact_analysis@format.rs"],
    ),
    (
        Bucket::Hierarchy,
        "indexer::write_parsed_file",
        &["write_parsed_file"],
    ),
    (
        Bucket::Hierarchy,
        "queries::find_symbol_fts",
        &["find_symbol_fts"],
    ),
    (
        Bucket::Hierarchy,
        "manifests::parse_go_mod",
        &["parse_go_mod"],
    ),
    (Bucket::Hierarchy, "traversal::bfs", &["bfs"]),
    (
        Bucket::Hierarchy,
        "dead_code::path_looks_like_test",
        &["path_looks_like_test"],
    ),
    (
        Bucket::Hierarchy,
        "SemanticModel::get",
        &["get@embedder.rs"],
    ),
    (
        Bucket::Hierarchy,
        "LanguageRegistry::for_extension",
        &["for_extension"],
    ),
    (
        Bucket::Hierarchy,
        "LanguageRegistry.register",
        &["register@mct-core"],
    ),
    (Bucket::Hierarchy, "ttc::parse", &["parse@ttc.rs"]),
    (Bucket::Hierarchy, "file_tree::build_node", &["build_node"]),
    (
        Bucket::Hierarchy,
        "MctServer::get_file_tree",
        &["get_file_tree@server.rs"],
    ),
    (
        Bucket::Hierarchy,
        "Walker::visit_definition",
        &["visit_definition"],
    ),
    (
        Bucket::Hierarchy,
        "RustParser::parse",
        &["parse@mct-lang-rust"],
    ),
    (
        Bucket::Hierarchy,
        "GoParser.file_extensions",
        &["file_extensions@mct-lang-go"],
    ),
    (
        Bucket::Hierarchy,
        "background::spawn_watcher",
        &["spawn_watcher"],
    ),
    (
        Bucket::Hierarchy,
        "mct_core::SymbolRecord",
        &["SymbolRecord"],
    ),
    (
        Bucket::Hierarchy,
        "LocalEmbedder::embed",
        &["embed@embedder.rs"],
    ),
    (
        Bucket::Hierarchy,
        "python Walker push_relation",
        &["push_relation@mct-lang-python"],
    ),
    (
        Bucket::Hierarchy,
        "MctServer::find_dead_code",
        &["find_dead_code@server.rs"],
    ),
    (
        Bucket::Hierarchy,
        "QueryScope::is_empty",
        &["is_empty@queries.rs"],
    ),
    (
        Bucket::Hierarchy,
        "exclude::read_ignore_file",
        &["read_ignore_file"],
    ),
];

/// `"double-quoted"` exact-phrase queries and the target (same notation as
/// [`CASES`]) that must rank first. Phrase mode forces `alpha` to 0, so
/// these run without the embedding model.
const PHRASE_CASES: &[(&str, &str)] = &[
    // Literal names, in their own spelling or another style.
    ("\"read_ignore_file\"", "read_ignore_file"),
    ("\"DEFAULT_EXCLUDE_DIRS\"", "DEFAULT_EXCLUDE_DIRS"),
    ("\"first_error\"", "first_error"),
    ("\"split identifier\"", "split_identifier"),
    ("\"fanInCounts\"", "fan_in_counts"),
    // Error-message-like sentences, punctuation included.
    (
        "\"syntax error is reported, not panicked\"",
        "syntax_error_is_reported_not_panicked",
    ),
    (
        "\"missing field is an error (not a panic)\"",
        "missing_field_is_an_error_not_a_panic",
    ),
    (
        "\"No match in any mode returns empty, not an error.\"",
        "no_match_in_any_mode_returns_empty_not_an_error",
    ),
    (
        "\"malformed OR operator-laden queries never error\"",
        "malformed_or_operator_laden_queries_never_error",
    ),
    // Special characters: qualifiers, call parens, FTS5 operator chars.
    (
        "\"Index::hybrid_search\"",
        "hybrid_search@mct-index/src/lib.rs",
    ),
    ("\"mct_core::SymbolRecord\"", "SymbolRecord"),
    ("\"toon.decode_table\"", "decode_table"),
    ("\"ExcludeSet::is_excluded\"", "is_excluded"),
    ("\"dot_encoded()\"", "dot_encoded"),
    ("\"index_error()\"", "index_error"),
    ("\"`unix_now`;\"", "unix_now"),
    ("\"file_tree_error*\"", "file_tree_error"),
];

fn is_target(hit: &HybridHit, expected: &[&str]) -> bool {
    expected.iter().any(|target| match target.split_once('@') {
        Some((name, fragment)) => hit.hit.name == name && hit.hit.relative_path.contains(fragment),
        None => hit.hit.name == *target,
    })
}

#[derive(Default)]
struct Score {
    hits: f64,
    rr: f64,
    n: f64,
}

impl Score {
    fn recall(&self) -> f64 {
        self.hits / self.n.max(1.0)
    }
    fn mrr(&self) -> f64 {
        self.rr / self.n.max(1.0)
    }
}

struct Run {
    overall: Score,
    buckets: Vec<(Bucket, Score)>,
    latencies: Vec<Duration>,
}

impl Run {
    fn percentile(&self, p: f64) -> Duration {
        let mut sorted = self.latencies.clone();
        sorted.sort();
        let at = ((sorted.len() as f64 - 1.0) * p).round() as usize;
        sorted.get(at).copied().unwrap_or_default()
    }

    fn print(&self, label: &str) {
        let per_bucket: Vec<String> = self
            .buckets
            .iter()
            .map(|(b, s)| format!("{b:?} {:.2}/{:.3}", s.recall(), s.mrr()))
            .collect();
        println!(
            "{label:<9} recall@10 {:.3}  MRR {:.3}  [{}]  p50 {:?} p95 {:?} max {:?}",
            self.overall.recall(),
            self.overall.mrr(),
            per_bucket.join(", "),
            self.percentile(0.5),
            self.percentile(0.95),
            self.percentile(1.0),
        );
    }
}

/// `alpha: None` routes each query through [`classify_query`], as the MCP
/// tool does when the caller passes no `alpha`.
fn score(index: &Index, model: &SemanticModel, alpha: Option<f64>) -> Run {
    let embedder = model.get(index.root()).unwrap();
    let scope = QueryScope {
        path: Some("crates"),
        language: None,
    };
    let mut run = Run {
        overall: Score::default(),
        buckets: BUCKETS.iter().map(|b| (*b, Score::default())).collect(),
        latencies: Vec::new(),
    };
    for (bucket, query, expected) in CASES {
        let alpha = alpha.unwrap_or_else(|| classify_query(query).alpha());
        let started = Instant::now();
        let hits = index
            .hybrid_search(query, Some(embedder), alpha, scope)
            .unwrap();
        run.latencies.push(started.elapsed());
        let rank = hits.iter().take(10).position(|h| is_target(h, expected));
        let (hit, rr) = match rank {
            Some(rank) => (1.0, 1.0 / (rank as f64 + 1.0)),
            None => {
                println!("  alpha {alpha}: miss [{bucket:?}] `{query}`");
                (0.0, 0.0)
            }
        };
        for score in [
            &mut run.overall,
            &mut run.buckets.iter_mut().find(|(b, _)| b == bucket).unwrap().1,
        ] {
            score.hits += hit;
            score.rr += rr;
            score.n += 1.0;
        }
    }
    run
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
    println!(
        "{} cases; embedded {embedded} symbols with {} in {:?}",
        CASES.len(),
        embedder.model_id(),
        started.elapsed()
    );

    let lexical = score(&index, &model, Some(0.0));
    let semantic = score(&index, &model, Some(1.0));
    let fixed = score(&index, &model, Some(0.5));
    let hybrid = score(&index, &model, None);
    lexical.print("lexical");
    semantic.print("semantic");
    fixed.print("alpha 0.5");
    hybrid.print("routed");

    // The #31 bar: routed hybrid finds more of the targets in its top 10
    // than either side alone, and ranks them better than lexical alone.
    assert!(hybrid.overall.recall() > lexical.overall.recall());
    assert!(hybrid.overall.recall() > semantic.overall.recall());
    assert!(hybrid.overall.mrr() > lexical.overall.mrr());
    // The #63 bar: recall@10 above 0.85 within a 15 ms warm p95 on CPU
    // (last run, bge-small-en-v1.5: recall 0.96, MRR 0.78, p95 ~7 ms).
    assert!(
        hybrid.overall.recall() > 0.85,
        "recall@10 {:.3}",
        hybrid.overall.recall()
    );
    assert!(
        hybrid.percentile(0.95) < Duration::from_millis(15),
        "p95 query took {:?}",
        hybrid.percentile(0.95)
    );
}

#[test]
fn exact_phrases_rank_their_target_first() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let scope = QueryScope {
        path: Some("crates"),
        language: None,
    };
    let mut misses = Vec::new();
    for (query, target) in PHRASE_CASES {
        assert_eq!(classify_query(query).alpha(), 0.0, "{query}");
        // alpha 1 asks for semantic only; the quotes must override it.
        let hits = index.hybrid_search(query, None, 1.0, scope).unwrap();
        assert!(hits.iter().all(|h| h.semantic_rank.is_none()), "{query}");
        match hits.first() {
            Some(top) if is_target(top, &[target]) => {}
            top => misses.push(format!(
                "`{query}`: expected {target}, got {:?}",
                top.map(|h| (&h.hit.name, &h.hit.relative_path))
            )),
        }
    }
    println!(
        "{} exact-phrase cases, {} top-1 misses",
        PHRASE_CASES.len(),
        misses.len()
    );
    assert!(misses.is_empty(), "{misses:#?}");
}
