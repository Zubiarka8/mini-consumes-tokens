//! Query-cache benchmark (issue #16), separate from the functional tests in
//! `tests/query_cache.rs`: indexes a real project (this repository by
//! default), then measures, per path and never mixed,
//!
//! - **exact**: `search_symbols` and `hybrid_search` (lexical) repeated
//!   verbatim — uncached vs exact-hit tokens, and the cache's own lookup
//!   latency (validation included) against the <10 ms target;
//! - **semantic**: `hybrid_search` paraphrase pairs — uncached tokens of the
//!   paraphrase vs its semantic-hit reply, plus embedding and validation
//!   latency.
//!
//! Token reduction is `1 - cached / uncached` with the project's usual
//! `chars / 4` estimate. Exits non-zero when the exact p95 lookup is ≥10 ms or
//! a path's aggregate reduction is <90%.
//!
//! ```sh
//! cargo run --release -p mct-mcp-server --example cache_benchmark [-- <root>]
//! cargo run --release -p mct-mcp-server --features semantic --example cache_benchmark
//! ```
//!
//! Without `--features semantic` the semantic path runs on a deterministic
//! bag-of-words embedder (content words only, so a paraphrase differing in
//! stop words embeds identically) — it measures the cache's cost and token
//! accounting, not a model's notion of similarity. With the feature, the real
//! model decides which paraphrases clear the 0.95 threshold, and misses are
//! reported as such.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mct_index::{Embedder, ExcludeSet, Index};
use mct_mcp_server::cache::{CacheConfig, CacheStats};
use mct_mcp_server::server::{HybridSearchArgs, MctServer, SearchSymbolsArgs, ServerOptions};
use rmcp::handler::server::wrapper::Parameters;

const REPEATS: usize = 30;
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "of", "to", "from", "for", "in", "on", "all",
];

const LEXICAL_QUERIES: &[&str] = &[
    "parse",
    "reindex",
    "search symbols",
    "embed query",
    "file tree",
    "context pack",
    "dead code",
    "tool catalog",
    "exclude set",
    "hybrid search",
];

const PARAPHRASES: &[(&str, &str)] = &[
    ("load settings from disk", "load the settings from disk"),
    ("find unused functions", "find all the unused functions"),
    ("parse a manifest file", "parse the manifest file"),
    ("walk the call graph", "walk a call graph"),
    (
        "split identifier into words",
        "split an identifier into the words",
    ),
    (
        "rank symbols by similarity",
        "rank the symbols by similarity",
    ),
];

/// Hashed bag of content words — see the module docs.
struct BagOfWords;

impl BagOfWords {
    fn vector(text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; 64];
        for word in text
            .split(|c: char| !c.is_alphanumeric())
            .map(str::to_lowercase)
            .filter(|w| !w.is_empty() && !STOP_WORDS.contains(&w.as_str()))
        {
            let h = word.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
            });
            v[(h % 64) as usize] += 1.0;
        }
        v
    }
}

impl Embedder for BagOfWords {
    fn model_id(&self) -> &str {
        "bench/bag-of-words"
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Ok(texts.iter().map(|t| Self::vector(t)).collect())
    }
}

fn embedder() -> Option<Box<dyn Embedder>> {
    if cfg!(feature = "semantic") {
        None // the build's real model, loaded lazily
    } else {
        Some(Box::new(BagOfWords))
    }
}

fn build(root: &Path, cache: CacheConfig) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::with_options(
        index,
        registry,
        ServerOptions {
            cache,
            embedder: embedder(),
            shared_cache: None,
        },
    )
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

fn text(result: rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|b| b.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

fn tokens(text: &str) -> f64 {
    text.chars().count() as f64 / 4.0
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn percentile(samples: &mut [Duration], p: f64) -> Duration {
    samples.sort();
    let i = ((samples.len() as f64 - 1.0) * p).round() as usize;
    samples.get(i).copied().unwrap_or_default()
}

/// The lookup latency one call added to the cache's counters.
fn lookup_delta(before: &CacheStats, after: &CacheStats) -> Option<Duration> {
    (after.lookup_latency.count > before.lookup_latency.count)
        .then(|| after.lookup_latency.total - before.lookup_latency.total)
}

#[derive(Default)]
struct PathReport {
    uncached_tokens: f64,
    cached_tokens: f64,
    hits: usize,
    misses: Vec<String>,
    uncached_call: Vec<Duration>,
    hit_call: Vec<Duration>,
    lookup: Vec<Duration>,
}

impl PathReport {
    fn reduction(&self) -> f64 {
        if self.uncached_tokens == 0.0 {
            0.0
        } else {
            1.0 - self.cached_tokens / self.uncached_tokens
        }
    }

    fn print(&mut self, name: &str) {
        println!("\n## {name} cache");
        println!(
            "hits {}/{}{}",
            self.hits,
            self.hits + self.misses.len(),
            if self.misses.is_empty() {
                String::new()
            } else {
                format!(" — misses: {}", self.misses.join(", "))
            }
        );
        println!(
            "tokens (valid hits only): uncached {:.0}, cached {:.0} → reduction {:.1}%",
            self.uncached_tokens,
            self.cached_tokens,
            self.reduction() * 100.0
        );
        for (label, samples) in [
            ("uncached call", &mut self.uncached_call),
            ("cache-hit call (end to end)", &mut self.hit_call),
            ("cache lookup (incl. validation)", &mut self.lookup),
        ] {
            if samples.is_empty() {
                continue;
            }
            println!(
                "{label}: p50 {:.3} ms, p95 {:.3} ms, max {:.3} ms (n={})",
                ms(percentile(samples, 0.50)),
                ms(percentile(samples, 0.95)),
                ms(percentile(samples, 1.0)),
                samples.len()
            );
        }
    }
}

#[tokio::main]
async fn main() {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .canonicalize()
        .unwrap();
    println!("# Query cache benchmark — {}", root.display());
    let start = Instant::now();
    let cached = build(&root, CacheConfig::default());
    let uncached = build(&root, CacheConfig::disabled());
    println!(
        "indexed twice in {:.1} s; semantic side: {}",
        start.elapsed().as_secs_f64(),
        if cfg!(feature = "semantic") {
            "the build's embedding model"
        } else {
            "deterministic bag-of-words embedder (no --features semantic)"
        }
    );

    // ---- exact ----
    let mut exact = PathReport::default();
    for &query in LEXICAL_QUERIES {
        for lexical_hybrid in [false, true] {
            let call = |server: &MctServer| {
                let server = server.clone();
                async move {
                    if lexical_hybrid {
                        text(
                            server
                                .hybrid_search(Parameters(hybrid_args(query, 0.0)))
                                .await
                                .unwrap(),
                        )
                    } else {
                        text(
                            server
                                .search_symbols(Parameters(search_args(query)))
                                .await
                                .unwrap(),
                        )
                    }
                }
            };
            let t = Instant::now();
            let full = call(&cached).await;
            exact.uncached_call.push(t.elapsed());
            for i in 0..REPEATS {
                let before = cached.cache_stats();
                let t = Instant::now();
                let reply = call(&cached).await;
                exact.hit_call.push(t.elapsed());
                let after = cached.cache_stats();
                exact.lookup.extend(lookup_delta(&before, &after));
                if i == 0 {
                    if after.exact_hit > before.exact_hit {
                        exact.hits += 1;
                        exact.uncached_tokens += tokens(&full);
                        exact.cached_tokens += tokens(&reply);
                    } else {
                        exact.misses.push(query.to_string());
                    }
                }
            }
        }
    }

    // ---- semantic ----
    let mut semantic = PathReport::default();
    for &(source, paraphrase) in PARAPHRASES {
        let _ = cached
            .hybrid_search(Parameters(hybrid_args(source, 0.75)))
            .await
            .unwrap();
        let t = Instant::now();
        let full = text(
            uncached
                .hybrid_search(Parameters(hybrid_args(paraphrase, 0.75)))
                .await
                .unwrap(),
        );
        semantic.uncached_call.push(t.elapsed());
        let before = cached.cache_stats();
        let t = Instant::now();
        let reply = text(
            cached
                .hybrid_search(Parameters(hybrid_args(paraphrase, 0.75)))
                .await
                .unwrap(),
        );
        semantic.hit_call.push(t.elapsed());
        let after = cached.cache_stats();
        // The semantic lookup is the second lookup of the call (after the
        // exact miss); the delta covers both, which is what the call paid.
        semantic.lookup.extend(lookup_delta(&before, &after));
        if after.semantic_hit > before.semantic_hit {
            semantic.hits += 1;
            semantic.uncached_tokens += tokens(&full);
            semantic.cached_tokens += tokens(&reply);
        } else {
            semantic.misses.push(format!("`{paraphrase}`"));
        }
    }

    exact.print("Exact");
    semantic.print("Semantic");
    let stats = cached.cache_stats();
    println!("\n## Counters\n{stats}");

    let exact_p95 = percentile(&mut exact.lookup, 0.95);
    let mut failures = Vec::new();
    if exact_p95 >= Duration::from_millis(10) {
        failures.push(format!("exact lookup p95 {:.3} ms ≥ 10 ms", ms(exact_p95)));
    }
    if exact.hits > 0 && exact.reduction() < 0.90 {
        failures.push(format!(
            "exact reduction {:.1}% < 90%",
            exact.reduction() * 100.0
        ));
    }
    if semantic.hits > 0 && semantic.reduction() < 0.90 {
        failures.push(format!(
            "semantic reduction {:.1}% < 90%",
            semantic.reduction() * 100.0
        ));
    }
    if failures.is_empty() {
        println!(
            "\nPASS: exact p95 lookup < 10 ms, ≥90% token reduction on each path's valid hits"
        );
    } else {
        println!("\nFAIL: {}", failures.join("; "));
        std::process::exit(1);
    }
}
