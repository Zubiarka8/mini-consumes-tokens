//! The embedding model behind `hybrid_search`'s semantic half (issue #61).
//!
//! Only compiled in with the `semantic` cargo feature, which pulls in
//! `fastembed` (a local ONNX runtime running `bge-small-en-v1.5`, 384
//! dimensions, by default — `MCT_EMBEDDING_MODEL` picks another of
//! [`MODELS`]; see #63 for how they compare on this repo's retrieval
//! benchmark). The model is loaded lazily on the first `hybrid_search`
//! call that needs it — downloaded once into `$FASTEMBED_CACHE_DIR`, or
//! `<root>/.mct-index/models` by default — so a session that never asks for
//! semantic search never pays for it. Symbol names never leave the machine.
//!
//! Without the feature, or when loading fails (e.g. offline on first use),
//! [`SemanticModel::get`] reports why and `hybrid_search` degrades to the
//! lexical ranking instead of erroring. A failed load is remembered for the
//! rest of the session rather than retried on every call.

use std::path::Path;
use std::sync::OnceLock;

use mct_index::Embedder;

#[derive(Default)]
pub struct SemanticModel {
    loaded: OnceLock<Result<Box<dyn Embedder>, String>>,
}

impl SemanticModel {
    /// The loaded model, loading it on first call, or why it's unavailable.
    pub fn get(&self, root: &Path) -> Result<&dyn Embedder, &str> {
        match self.loaded.get_or_init(|| load(root)) {
            Ok(embedder) => Ok(embedder.as_ref()),
            Err(reason) => Err(reason.as_str()),
        }
    }
}

#[cfg(not(feature = "semantic"))]
fn load(_root: &Path) -> Result<Box<dyn Embedder>, String> {
    Err(
        "this build has no embedding model (rebuild mct-mcp-server with `--features semantic`)"
            .to_string(),
    )
}

/// Environment variable selecting the embedding model by its short name
/// (see [`MODELS`]); unset means the first entry.
pub const MODEL_ENV: &str = "MCT_EMBEDDING_MODEL";

/// The query instruction BGE / Arctic retrieval models were trained with.
#[cfg(feature = "semantic")]
const RETRIEVAL_QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";

/// (short name, fastembed model, query prefix). The first is the default:
/// on `crates/mct-mcp-server/tests/hybrid_benchmark.rs` it had the best
/// recall@10 and MRR of these while staying well inside the latency budget
/// (all-MiniLM-L6-v2: slightly lower recall, ~2x faster; jina-code: too
/// slow per query for the budget, and no better).
#[cfg(feature = "semantic")]
pub const MODELS: &[(&str, fastembed::EmbeddingModel, Option<&str>)] = &[
    (
        "bge-small-en-v1.5",
        fastembed::EmbeddingModel::BGESmallENV15,
        Some(RETRIEVAL_QUERY_PREFIX),
    ),
    (
        "all-MiniLM-L6-v2",
        fastembed::EmbeddingModel::AllMiniLML6V2,
        None,
    ),
    (
        "all-MiniLM-L12-v2",
        fastembed::EmbeddingModel::AllMiniLML12V2,
        None,
    ),
    (
        "snowflake-arctic-embed-xs",
        fastembed::EmbeddingModel::SnowflakeArcticEmbedXS,
        Some(RETRIEVAL_QUERY_PREFIX),
    ),
    (
        "jina-embeddings-v2-base-code",
        fastembed::EmbeddingModel::JinaEmbeddingsV2BaseCode,
        None,
    ),
];

#[cfg(feature = "semantic")]
fn load(root: &Path) -> Result<Box<dyn Embedder>, String> {
    use fastembed::{TextEmbedding, TextInitOptions};

    let wanted = std::env::var(MODEL_ENV).unwrap_or_default();
    let wanted = wanted.trim();
    let (name, model, query_prefix) = MODELS
        .iter()
        .find(|(name, _, _)| wanted.is_empty() || name.eq_ignore_ascii_case(wanted))
        .ok_or_else(|| {
            let known: Vec<&str> = MODELS.iter().map(|(name, _, _)| *name).collect();
            format!(
                "unknown {MODEL_ENV} `{wanted}` (known: {})",
                known.join(", ")
            )
        })?;
    let cache_dir = std::env::var_os("FASTEMBED_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join(".mct-index").join("models"));
    let options = TextInitOptions::new(model.clone())
        .with_cache_dir(cache_dir)
        .with_show_download_progress(false);
    let model = TextEmbedding::try_new(options)
        .map_err(|e| format!("could not load the embedding model: {e}"))?;
    Ok(Box::new(LocalEmbedder {
        id: format!("fastembed/{name}"),
        query_prefix: *query_prefix,
        model: std::sync::Mutex::new(model),
    }))
}

/// `fastembed`'s `embed` takes `&mut self`; the mutex makes it shareable.
#[cfg(feature = "semantic")]
struct LocalEmbedder {
    id: String,
    query_prefix: Option<&'static str>,
    model: std::sync::Mutex<fastembed::TextEmbedding>,
}

#[cfg(feature = "semantic")]
impl Embedder for LocalEmbedder {
    fn model_id(&self) -> &str {
        &self.id
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let mut model = self
            .model
            .lock()
            .map_err(|_| "embedding model lock poisoned".to_string())?;
        model.embed(texts, None).map_err(|e| e.to_string())
    }

    fn embed_query(&self, query: &str) -> Result<Vec<f32>, String> {
        let text = format!("{}{query}", self.query_prefix.unwrap_or_default());
        self.embed(&[text])?
            .pop()
            .ok_or_else(|| "embedder returned no vector for the query".to_string())
    }
}
