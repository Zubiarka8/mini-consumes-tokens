//! The embedding model behind `hybrid_search`'s semantic half (issue #61).
//!
//! Only compiled in with the `semantic` cargo feature, which pulls in
//! `fastembed` (a local ONNX runtime running `all-MiniLM-L6-v2`, 384
//! dimensions). The model is loaded lazily on the first `hybrid_search`
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

#[cfg(feature = "semantic")]
fn load(root: &Path) -> Result<Box<dyn Embedder>, String> {
    use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

    let cache_dir = std::env::var_os("FASTEMBED_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join(".mct-index").join("models"));
    let options = TextInitOptions::new(EmbeddingModel::AllMiniLML6V2)
        .with_cache_dir(cache_dir)
        .with_show_download_progress(false);
    let model = TextEmbedding::try_new(options)
        .map_err(|e| format!("could not load the embedding model: {e}"))?;
    Ok(Box::new(LocalEmbedder {
        model: std::sync::Mutex::new(model),
    }))
}

/// `fastembed`'s `embed` takes `&mut self`; the mutex makes it shareable.
#[cfg(feature = "semantic")]
struct LocalEmbedder {
    model: std::sync::Mutex<fastembed::TextEmbedding>,
}

#[cfg(feature = "semantic")]
impl Embedder for LocalEmbedder {
    fn model_id(&self) -> &str {
        "fastembed/all-MiniLM-L6-v2"
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let mut model = self
            .model
            .lock()
            .map_err(|_| "embedding model lock poisoned".to_string())?;
        model.embed(texts, None).map_err(|e| e.to_string())
    }
}
