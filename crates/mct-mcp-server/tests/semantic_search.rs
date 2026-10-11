//! In-process tests of the `semantic_search` tool (issue #26) with a
//! deterministic bag-of-words embedder: ranking by cosine similarity, the
//! signature and score on each hit, scope, and re-embedding after the
//! vectors are dropped. The issue's acceptance checks with the real model
//! (retrieval and latency on 10K symbols) are the ignored
//! `real_model_finds_validate_token_among_10k_symbols` below.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use mct_index::{Embedder, ExcludeSet, Index};
use mct_mcp_server::cache::CacheConfig;
use mct_mcp_server::server::{MctServer, SemanticSearchArgs, ServerOptions};
use rmcp::handler::server::wrapper::Parameters;

const DIM: usize = 64;

/// Each lowercase word hashed into one of `DIM` buckets.
struct BagOfWords;

fn bag_of_words(text: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; DIM];
    for word in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let h = word
            .to_lowercase()
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
            });
        v[(h % DIM as u64) as usize] += 1.0;
    }
    v
}

impl Embedder for BagOfWords {
    fn model_id(&self) -> &str {
        "fake/bag-of-words"
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Ok(texts.iter().map(|t| bag_of_words(t)).collect())
    }
}

/// A throwaway project, unique per test (tests run in parallel).
fn project(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("mct-semantic-search-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/auth.rs"),
        "pub fn validate_token(token: &str) -> bool {\n    !token.is_empty()\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/billing.rs"),
        "pub fn create_invoice(customer: &str) -> String {\n    customer.to_string()\n}\n",
    )
    .unwrap();
    dir.canonicalize().unwrap()
}

fn server(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let options = ServerOptions {
        cache: CacheConfig::default(),
        embedder: Some(Box::new(BagOfWords)),
        shared_cache: None,
        tool_attention: false,
    };
    MctServer::with_options(index, registry, options)
}

async fn search(server: &MctServer, query: &str, path: Option<&str>) -> String {
    let result = server
        .semantic_search(Parameters(SemanticSearchArgs {
            query: query.to_string(),
            top_k: Some(1),
            path: path.map(str::to_string),
            language: None,
        }))
        .await
        .unwrap();
    result.content[0].as_text().unwrap().text.clone()
}

#[tokio::test]
async fn ranks_by_similarity_with_signature_score_and_scope() {
    let root = project("rank");
    let server = server(&root);

    let text = search(&server, "validate the user token", None).await;
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("semantic: "), "got: {text}");
    assert!(lines[0].contains("(fake/bag-of-words)"), "got: {text}");
    assert!(
        lines[2].starts_with("src/auth.rs:L1-L3 [rust] function validate_token  similarity 0."),
        "got: {text}"
    );
    assert_eq!(
        lines[3].trim(),
        "pub fn validate_token(token: &str) -> bool",
        "got: {text}"
    );

    // Scope wins over similarity: only billing.rs is searched.
    let scoped = search(&server, "validate the user token", Some("src/billing.rs")).await;
    assert!(scoped.contains("\nsrc/billing.rs:"), "got: {scoped}");
    assert!(!scoped.contains("src/auth.rs"), "got: {scoped}");

    std::fs::remove_dir_all(&root).ok();
}

/// A file's synthetic `module` symbol has no declaration line of its own:
/// it must not borrow the file's first line as its signature.
#[test]
fn a_whole_file_module_hit_has_no_signature() {
    let root = project("module");
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    index.refresh_embeddings(&BagOfWords).unwrap();
    let scope = mct_index::QueryScope {
        path: Some("src/billing.rs"),
        language: None,
    };
    let vector = Index::embed_query(&BagOfWords, "billing").unwrap();
    let hits = index
        .semantic_search(&vector, BagOfWords.model_id(), scope, 10)
        .unwrap();
    let module = hits.iter().find(|h| h.hit.kind == "module").unwrap();
    assert_eq!(module.signature, None, "{module:?}");
    let function = hits.iter().find(|h| h.hit.kind == "function").unwrap();
    assert!(function.signature.is_some(), "{function:?}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn cleared_embeddings_are_rebuilt_in_full() {
    let root = project("rebuild");
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let first = index.refresh_embeddings(&BagOfWords).unwrap();
    assert!(first > 0);
    assert_eq!(index.refresh_embeddings(&BagOfWords).unwrap(), 0);
    index.clear_embeddings().unwrap();
    assert_eq!(index.refresh_embeddings(&BagOfWords).unwrap(), first);
    std::fs::remove_dir_all(&root).ok();
}

/// Median and 95th percentile of `runs` timings of `f`.
#[cfg(feature = "semantic")]
fn timed(runs: usize, mut f: impl FnMut(usize)) -> (std::time::Duration, std::time::Duration) {
    let mut all: Vec<_> = (0..runs)
        .map(|i| {
            let t = std::time::Instant::now();
            f(i);
            t.elapsed()
        })
        .collect();
    all.sort();
    (all[runs / 2], all[runs * 95 / 100])
}

/// Issue #26's acceptance checks with the real local model
/// (`bge-small-en-v1.5` unless `MCT_EMBEDDING_MODEL` says otherwise):
/// `validate user authentication` must return `validate_token` in the top
/// 10 of a ~10K-symbol project, and the timings of the query embedding, the
/// similarity scan and the whole warm tool call are printed separately.
///
/// The project is generated: `src/auth.rs` (no doc comments) plus 20 modules
/// × 520 `verb_module_noun` functions. Auth-adjacent nouns (user, session,
/// password, permission, role, account, token) are among the distractors,
/// so 400 generated names such as `validate_admin_token` restate the query
/// at least as well as the answer's name does.
///
/// `cargo test --locked -p mct-mcp-server --features semantic --test semantic_search \
///    -- --ignored --nocapture real_model`
#[cfg(feature = "semantic")]
#[ignore = "not a bug: downloads the model on first use and embeds ~10K symbols"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_model_finds_validate_token_among_10k_symbols() {
    use std::fmt::Write as _;
    use std::time::Instant;

    const VERBS: &[&str] = &[
        "validate", "parse", "load", "save", "create", "delete", "update", "fetch", "send",
        "render", "compute", "encode", "decode", "check", "build", "reset", "merge", "sort",
        "filter", "format",
    ];
    const NOUNS: &[&str] = &[
        "user",
        "session",
        "config",
        "invoice",
        "order",
        "payment",
        "file",
        "cache",
        "request",
        "response",
        "password",
        "email",
        "report",
        "image",
        "record",
        "account",
        "permission",
        "role",
        "message",
        "queue",
        "schema",
        "route",
        "header",
        "price",
        "address",
        "token",
    ];
    const MODULES: &[&str] = &[
        "billing",
        "storage",
        "http",
        "mail",
        "admin",
        "search",
        "media",
        "export",
        "import",
        "metrics",
        "audit",
        "catalog",
        "checkout",
        "profile",
        "shipping",
        "inventory",
        "sync",
        "scheduler",
        "reports",
        "settings",
    ];

    let root = project("real-model");
    std::fs::write(
        root.join("src/auth.rs"),
        "pub struct Claims {\n    pub sub: String,\n    pub exp: u64,\n}\n\n\
         pub fn validate_token(token: &str, secret: &[u8]) -> Result<Claims, String> {\n    \
         if token.is_empty() || secret.is_empty() {\n        return Err(\"invalid\".into());\n    }\n    \
         Ok(Claims { sub: token.to_string(), exp: 0 })\n}\n\n\
         pub fn hash_password(password: &str) -> String {\n    password.chars().rev().collect()\n}\n\n\
         pub fn authenticate_user(name: &str, password: &str) -> bool {\n    \
         !name.is_empty() && hash_password(password).len() > 8\n}\n",
    )
    .unwrap();
    std::fs::remove_file(root.join("src/billing.rs")).unwrap();
    for module in MODULES {
        let mut source = String::new();
        for verb in VERBS {
            for noun in NOUNS {
                writeln!(
                    source,
                    "pub fn {verb}_{module}_{noun}(input: &str) -> usize {{\n    input.len()\n}}\n"
                )
                .unwrap();
            }
        }
        std::fs::write(root.join(format!("src/{module}.rs")), source).unwrap();
    }

    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let model = mct_mcp_server::embedder::SemanticModel::default();
    let t = Instant::now();
    let embedder = model.get(&root).unwrap();
    eprintln!("model load: {:?} ({})", t.elapsed(), embedder.model_id());
    let t = Instant::now();
    let embedded = index.refresh_embeddings(embedder).unwrap();
    let coverage = index.embedding_coverage(embedder.model_id()).unwrap();
    eprintln!(
        "cold embedding of every symbol: {:?} ({embedded} embedded, {}/{} covered)",
        t.elapsed(),
        coverage.embedded,
        coverage.total
    );
    assert!(coverage.total >= 10_000, "only {} symbols", coverage.total);

    let everywhere = mct_index::QueryScope {
        path: None,
        language: None,
    };
    let query = "validate user authentication";
    let vector = Index::embed_query(embedder, query).unwrap();
    let hits = index
        .semantic_search(&vector, embedder.model_id(), everywhere, 10)
        .unwrap();
    for (rank, h) in hits.iter().enumerate() {
        eprintln!("  #{} {:.3} {}", rank + 1, h.similarity, h.hit.name);
    }
    assert!(
        hits.iter().any(|h| h.hit.name == "validate_token"),
        "validate_token not in the top 10"
    );

    let queries = [
        query,
        "check a bearer token",
        "send an email message",
        "compute the order price",
        "load settings from disk",
    ];
    let (embed_median, embed_p95) = timed(30, |i| {
        Index::embed_query(embedder, queries[i % queries.len()]).unwrap();
    });
    let (scan_median, scan_p95) = timed(30, |_| {
        index
            .semantic_search(&vector, embedder.model_id(), everywhere, 10)
            .unwrap();
    });
    eprintln!("query embedding: median {embed_median:?}, p95 {embed_p95:?}");
    eprintln!("similarity scan + signatures, top 10: median {scan_median:?}, p95 {scan_p95:?}");

    // The whole tool call on a warm server (its own model instance, vectors
    // already stored): pending-symbol check + query embedding + scan + render.
    let server = MctServer::with_options(
        index,
        registry,
        ServerOptions {
            cache: CacheConfig::disabled(),
            embedder: None,
            shared_cache: None,
        },
    );
    let call = |q: &str| {
        let server = server.clone();
        let q = q.to_string();
        async move {
            server
                .semantic_search(Parameters(SemanticSearchArgs {
                    query: q,
                    top_k: Some(10),
                    path: None,
                    language: None,
                }))
                .await
                .unwrap()
        }
    };
    let t = Instant::now();
    let first = call(query).await;
    eprintln!(
        "first tool call (loads the server's model): {:?}",
        t.elapsed()
    );
    eprintln!("{}", first.content[0].as_text().unwrap().text);
    let mut warm = Vec::new();
    for i in 0..30 {
        let t = Instant::now();
        call(queries[i % queries.len()]).await;
        warm.push(t.elapsed());
    }
    warm.sort();
    eprintln!(
        "warm semantic_search tool call: median {:?}, p95 {:?}",
        warm[15], warm[28]
    );
    std::fs::remove_dir_all(&root).ok();
}
