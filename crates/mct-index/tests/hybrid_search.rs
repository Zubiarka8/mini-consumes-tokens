//! `Index::hybrid_search` / `Index::refresh_embeddings` (issue #61): the
//! semantic half of hybrid search, fused with `search_symbols` by weighted
//! Reciprocal Rank Fusion. Uses a deterministic fake `Embedder` that maps
//! synonym groups (`load`/`read`, `config`/`settings`, ...) onto shared
//! dimensions, so "meaning" matches are predictable without an ML model.
//!
//! Uses the same toy `fn NAME`-per-line fake parser as `symbol_search.rs`,
//! duplicated locally since Rust integration test files don't share code.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use mct_core::{Location, ParseError, ParsedFile, SourceFile, SymbolKind, SymbolRecord};
use mct_index::{Embedder, ExcludeSet, Index, QueryScope};

struct FakeParser;

impl mct_core::LanguageParser for FakeParser {
    fn language_id(&self) -> &'static str {
        "fake"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["fake"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parsed = ParsedFile::default();
        for (line_no, line) in file.contents.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            if parts.next() != Some("fn") {
                return Err(ParseError::Syntax {
                    path: file.relative_path.clone(),
                    line: line_no as u32 + 1,
                    message: format!("expected `fn`, got `{line}`"),
                });
            }
            let name = parts.next().unwrap_or_default().to_string();
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name,
                kind: SymbolKind::Function,
                location: Location {
                    line: line_no as u32 + 1,
                    column: 1,
                    byte_len: line.len() as u32,
                    end_line: Some(line_no as u32 + 1),
                },
                parent: None,
                level: None,
            });
        }
        Ok(parsed)
    }
}

fn registry() -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    registry
}

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-index-hybrid-search-test-{}", uuid_like()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn uuid_like() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Synonym groups share a dimension; any other word lands in a hashed
/// dimension of its own. Counts how many texts it has embedded.
struct FakeEmbedder {
    model: &'static str,
    embedded: AtomicUsize,
}

const GROUPS: &[&[&str]] = &[
    &["load", "read", "fetch"],
    &["config", "settings", "preferences"],
    &["disk", "file"],
    &["parse", "decode"],
    &["request", "query"],
    &["render", "draw", "paint"],
    &["delete", "remove", "erase"],
    &["user", "account"],
];
const DIMS: usize = 64;

impl FakeEmbedder {
    fn new(model: &'static str) -> Self {
        Self {
            model,
            embedded: AtomicUsize::new(0),
        }
    }

    fn vector(text: &str) -> Vec<f32> {
        let mut v = vec![0.0; DIMS];
        for word in text.split_whitespace() {
            let dim = GROUPS
                .iter()
                .position(|g| g.contains(&word))
                .unwrap_or_else(|| {
                    GROUPS.len()
                        + word.bytes().map(usize::from).sum::<usize>() % (DIMS - GROUPS.len())
                });
            v[dim] += 1.0;
        }
        v
    }
}

impl Embedder for FakeEmbedder {
    fn model_id(&self) -> &str {
        self.model
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.embedded.fetch_add(texts.len(), Ordering::Relaxed);
        Ok(texts.iter().map(|t| Self::vector(t)).collect())
    }
}

struct FailingEmbedder;

impl Embedder for FailingEmbedder {
    fn model_id(&self) -> &str {
        "broken"
    }

    fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Err("model unavailable".to_string())
    }
}

fn indexed(files: &[(&str, &str)]) -> (std::path::PathBuf, Index) {
    let dir = tempdir();
    for (path, contents) in files {
        fs::write(dir.join(path), contents).unwrap();
    }
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    (dir, index)
}

fn fixture() -> (std::path::PathBuf, Index) {
    indexed(&[
        (
            "app.fake",
            "fn readSettingsFile\nfn drawWidget\nfn eraseAccount\nfn loadConfig\n",
        ),
        (
            "net.fake",
            "fn decodeQuery\nfn loadBalancer\nfn configureLogger\n",
        ),
    ])
}

fn hybrid(index: &Index, query: &str, embedder: Option<&dyn Embedder>, alpha: f64) -> Vec<String> {
    index
        .hybrid_search(query, embedder, alpha, QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| h.hit.name)
        .collect()
}

#[test]
fn semantic_side_finds_what_no_name_word_matches() {
    let (_dir, index) = fixture();
    let embedder = FakeEmbedder::new("fake-v1");
    index.refresh_embeddings(&embedder).unwrap();

    // No name word of `readSettingsFile` is in the query: lexical misses it.
    assert!(index
        .search_symbols("fetch preferences disk", QueryScope::default())
        .unwrap()
        .is_empty());
    let hits = hybrid(&index, "fetch preferences disk", Some(&embedder), 0.5);
    assert_eq!(hits.first().map(String::as_str), Some("readSettingsFile"));

    let hits = hybrid(&index, "remove user", Some(&embedder), 1.0);
    assert_eq!(hits.first().map(String::as_str), Some("eraseAccount"));
}

#[test]
fn alpha_zero_or_no_embedder_is_exactly_the_lexical_ranking() {
    let (_dir, index) = fixture();
    let embedder = FakeEmbedder::new("fake-v1");
    index.refresh_embeddings(&embedder).unwrap();
    for query in ["load", "config", "decode query", "LOADCONFIG", "zzz"] {
        let lexical: Vec<String> = index
            .search_symbols(query, QueryScope::default())
            .unwrap()
            .into_iter()
            .map(|h| h.name)
            .collect();
        assert_eq!(
            hybrid(&index, query, Some(&embedder), 0.0),
            lexical,
            "{query}"
        );
        assert_eq!(hybrid(&index, query, None, 0.7), lexical, "{query}");
        assert_eq!(
            hybrid(&index, query, Some(&embedder), f64::NAN),
            lexical,
            "{query}"
        );
    }
}

#[test]
fn fusion_rewards_agreement_between_both_lists() {
    let (_dir, index) = fixture();
    let embedder = FakeEmbedder::new("fake-v1");
    index.refresh_embeddings(&embedder).unwrap();

    // Lexically `loadConfig` and `loadBalancer` both match `load`; only
    // `loadConfig` is also semantically close to "load settings".
    let hits = index
        .hybrid_search("load settings", Some(&embedder), 0.5, QueryScope::default())
        .unwrap();
    let pos = |name: &str| hits.iter().position(|h| h.hit.name == name).unwrap();
    assert!(pos("loadConfig") < pos("loadBalancer"));
    let top = &hits[0];
    assert!(top.lexical_rank.is_some() && top.semantic_rank.is_some());
    assert!(hits.windows(2).skip(1).all(|w| w[0].score >= w[1].score));
}

#[test]
fn an_exact_name_ranks_first_whatever_alpha() {
    let (_dir, index) = fixture();
    let embedder = FakeEmbedder::new("fake-v1");
    index.refresh_embeddings(&embedder).unwrap();
    for alpha in [0.0, 0.5, 1.0] {
        let hits = hybrid(&index, "loadbalancer", Some(&embedder), alpha);
        assert_eq!(
            hits.first().map(String::as_str),
            Some("loadBalancer"),
            "{alpha}"
        );
    }
}

#[test]
fn scope_applies_to_the_semantic_side_too() {
    let (_dir, index) = fixture();
    let embedder = FakeEmbedder::new("fake-v1");
    index.refresh_embeddings(&embedder).unwrap();
    let scope = QueryScope {
        path: Some("net.fake"),
        language: None,
    };
    let hits = index
        .hybrid_search("fetch preferences disk", Some(&embedder), 1.0, scope)
        .unwrap();
    assert!(hits.iter().all(|h| h.hit.relative_path == "net.fake"));
}

#[test]
fn refresh_is_incremental_and_follows_reindex() {
    let (dir, mut index) = fixture();
    let embedder = FakeEmbedder::new("fake-v1");
    assert_eq!(index.refresh_embeddings(&embedder).unwrap(), 7);
    assert_eq!(index.refresh_embeddings(&embedder).unwrap(), 0);
    let coverage = index.embedding_coverage("fake-v1").unwrap();
    assert_eq!((coverage.embedded, coverage.total), (7, 7));

    // Modify one file, add one, remove one: only rewritten/new symbols are
    // re-embedded, and removed symbols' vectors disappear with them.
    fs::write(
        dir.join("app.fake"),
        "fn readSettingsFile\nfn paintCanvas\n",
    )
    .unwrap();
    fs::write(dir.join("extra.fake"), "fn fetchUser\n").unwrap();
    fs::remove_file(dir.join("net.fake")).unwrap();
    index.reindex(&registry(), false).unwrap();

    let before = embedder.embedded.load(Ordering::Relaxed);
    let pending = index.refresh_embeddings(&embedder).unwrap();
    assert_eq!(embedder.embedded.load(Ordering::Relaxed) - before, pending);
    assert!(
        pending <= 3,
        "only app.fake + extra.fake symbols, got {pending}"
    );
    let coverage = index.embedding_coverage("fake-v1").unwrap();
    assert_eq!((coverage.embedded, coverage.total), (3, 3));

    let hits = hybrid(&index, "draw", Some(&embedder), 1.0);
    assert_eq!(hits.first().map(String::as_str), Some("paintCanvas"));
    assert!(!hits.iter().any(|n| n == "drawWidget" || n == "decodeQuery"));
}

#[test]
fn switching_models_replaces_every_vector() {
    let (_dir, index) = fixture();
    index
        .refresh_embeddings(&FakeEmbedder::new("fake-v1"))
        .unwrap();
    assert_eq!(
        index
            .refresh_embeddings(&FakeEmbedder::new("fake-v2"))
            .unwrap(),
        7
    );
    assert_eq!(index.embedding_coverage("fake-v1").unwrap().embedded, 0);
    assert_eq!(index.embedding_coverage("fake-v2").unwrap().embedded, 7);
}

#[test]
fn a_failing_embedder_is_an_error_not_a_panic() {
    let (_dir, index) = fixture();
    assert!(index.refresh_embeddings(&FailingEmbedder).is_err());
    assert!(index
        .hybrid_search("load", Some(&FailingEmbedder), 0.5, QueryScope::default())
        .is_err());
    // ...but alpha = 0 never calls it.
    assert!(index
        .hybrid_search("load", Some(&FailingEmbedder), 0.0, QueryScope::default())
        .is_ok());
}

#[test]
fn vectors_survive_reopening_an_on_disk_index() {
    let dir = tempdir();
    fs::write(dir.join("app.fake"), "fn readSettingsFile\n").unwrap();
    let db = dir.join("index.sqlite3");
    let embedder = FakeEmbedder::new("fake-v1");
    {
        let mut index = Index::open(&dir, &db, ExcludeSet::default()).unwrap();
        index.reindex(&registry(), false).unwrap();
        index.refresh_embeddings(&embedder).unwrap();
    }
    let index = Index::open(&dir, &db, ExcludeSet::default()).unwrap();
    assert_eq!(index.refresh_embeddings(&embedder).unwrap(), 0);
    let hits = hybrid(&index, "load preferences", Some(&embedder), 1.0);
    assert_eq!(hits.first().map(String::as_str), Some("readSettingsFile"));
}
