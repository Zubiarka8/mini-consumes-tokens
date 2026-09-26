//! Semantic half of hybrid search (Phase 2 of #31): per-symbol embedding
//! vectors, a brute-force cosine ranking over them, and Reciprocal Rank
//! Fusion with the lexical [`crate::search::search_symbols`] ranking.
//!
//! This crate stays free of any ML dependency: vectors come from an
//! [`Embedder`] the caller supplies (`mct-mcp-server`'s `semantic` feature
//! provides a local ONNX model). Without one, [`hybrid_search`] degrades to
//! the lexical ranking unchanged.
//!
//! Vectors are stored L2-normalised as little-endian `f32` BLOBs in
//! `symbol_embeddings`, keyed by `symbols.id` with `ON DELETE CASCADE`, so a
//! reindex that drops or rewrites a symbol drops its vector with it;
//! [`refresh_embeddings`] then fills in whatever is missing. A plain scan is
//! used instead of a vector index (`sqlite-vec`): a few thousand to a few
//! hundred thousand 384-dimension dot products fit the latency target
//! without loading a SQLite extension.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::queries::{push_scope, BoundValues, ResolvedScope, SymbolHit};
use crate::search::{search_symbols, split_identifier};
use crate::{IndexError, Result};

/// RRF's rank-damping constant. Lower than the paper's usual 60: the
/// lexical list for a plain-language query is long and noisy, and a small
/// `k` lets each list's top few hits count for clearly more than its tail.
/// Tuned on `mct-mcp-server/tests/hybrid_benchmark.rs` (60, 20, 10, 5 tried).
pub const RRF_K: f64 = 10.0;

/// How many nearest symbols the semantic side contributes to the fusion.
/// Hits past this rank would add at most `alpha / (RRF_K + 200)` — noise.
pub const SEMANTIC_CANDIDATES: usize = 200;

/// Symbols embedded per [`Embedder::embed`] call during
/// [`refresh_embeddings`], and per committed transaction.
const EMBED_BATCH: usize = 256;

/// Turns text into vectors. Implemented outside this crate (see
/// `mct-mcp-server`'s `semantic` feature); tests use a deterministic fake.
pub trait Embedder: Send + Sync {
    /// Stable identifier of the model; stored per vector so switching models
    /// re-embeds instead of mixing incompatible vector spaces.
    fn model_id(&self) -> &str;

    /// One vector per input text, all of the same dimension, in input order.
    fn embed(&self, texts: &[String]) -> std::result::Result<Vec<Vec<f32>>, String>;
}

/// One fused `hybrid_search` hit, with the per-list ranks it was built from
/// (1-based; `None` when that list didn't contain it).
#[derive(Debug, Clone)]
pub struct HybridHit {
    pub hit: SymbolHit,
    pub score: f64,
    pub lexical_rank: Option<usize>,
    pub semantic_rank: Option<usize>,
}

/// How many symbols have a vector for `model`, out of how many exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingCoverage {
    pub embedded: usize,
    pub total: usize,
}

/// The text a symbol is embedded from: its kind and split name words, its
/// parent's words, and its file's stem as context —
/// `method parse request body in http parser (server)`. Split words rather
/// than the raw identifier, because sentence-embedding models tokenise
/// `parseRequestBody` into fragments that carry little meaning; the file
/// stem (`manifests`, `exclude`) says what area a terse name like
/// `parse_go_mod` belongs to, which measurably lifts semantic recall.
pub fn embedding_text(name: &str, kind: &str, parent: Option<&str>, relative_path: &str) -> String {
    let words = split_identifier(name).join(" ");
    let mut text = match parent.map(split_identifier).filter(|p| !p.is_empty()) {
        Some(parent_words) => format!("{kind} {words} in {}", parent_words.join(" ")),
        None => format!("{kind} {words}"),
    };
    let stem = relative_path
        .rsplit('/')
        .next()
        .and_then(|file| file.split('.').next())
        .map(split_identifier)
        .unwrap_or_default();
    if !stem.is_empty() {
        text.push_str(&format!(" ({})", stem.join(" ")));
    }
    text
}

/// Embeds every symbol that has no vector for `embedder`'s model yet (new or
/// rewritten since the last call), and drops vectors from any other model.
/// Returns how many symbols were embedded. Incremental: after the first
/// full pass, a call only pays for what the last reindex changed.
pub fn refresh_embeddings(conn: &Connection, embedder: &dyn Embedder) -> Result<usize> {
    let model = embedder.model_id();
    conn.execute("DELETE FROM symbol_embeddings WHERE model <> ?1", [model])?;

    let pending: Vec<(i64, String)> = {
        let mut stmt = conn.prepare_cached(
            "SELECT s.id, s.name, s.kind, s.parent, f.relative_path
             FROM symbols s
             JOIN files f ON f.id = s.file_id
             LEFT JOIN symbol_embeddings e ON e.symbol_id = s.id
             WHERE e.symbol_id IS NULL
             ORDER BY s.id",
        )?;
        let rows = stmt.query_map([], |row| {
            let name: String = row.get(1)?;
            let kind: String = row.get(2)?;
            let parent: Option<String> = row.get(3)?;
            let path: String = row.get(4)?;
            Ok((
                row.get(0)?,
                embedding_text(&name, &kind, parent.as_deref(), &path),
            ))
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };

    for chunk in pending.chunks(EMBED_BATCH) {
        let texts: Vec<String> = chunk.iter().map(|(_, text)| text.clone()).collect();
        let vectors = embedder.embed(&texts).map_err(IndexError::Embedding)?;
        if vectors.len() != chunk.len() {
            return Err(IndexError::Embedding(format!(
                "embedder returned {} vectors for {} texts",
                vectors.len(),
                chunk.len()
            )));
        }
        let tx = conn.unchecked_transaction()?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO symbol_embeddings(symbol_id, model, vector)
                 VALUES (?1, ?2, ?3)",
            )?;
            for ((id, _), vector) in chunk.iter().zip(vectors) {
                insert.execute(rusqlite::params![id, model, encode(&normalized(vector))])?;
            }
        }
        tx.commit()?;
    }
    Ok(pending.len())
}

/// Embedded vs. total symbol counts for `model`.
pub fn embedding_coverage(conn: &Connection, model: &str) -> Result<EmbeddingCoverage> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))?;
    let embedded: i64 = conn.query_row(
        "SELECT COUNT(*) FROM symbol_embeddings WHERE model = ?1",
        [model],
        |r| r.get(0),
    )?;
    Ok(EmbeddingCoverage {
        embedded: usize::try_from(embedded).unwrap_or(0),
        total: usize::try_from(total).unwrap_or(0),
    })
}

/// The `limit` symbols (within `scope`) whose stored vector is closest to
/// `query_vector` by cosine similarity, best first. Rows of another model or
/// dimension are ignored.
pub fn semantic_ranking(
    conn: &Connection,
    query_vector: &[f32],
    model: &str,
    scope: ResolvedScope<'_>,
    limit: usize,
) -> Result<Vec<(f32, SymbolHit)>> {
    let query = normalized(query_vector.to_vec());
    let mut sql = String::from(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line, s.level,
                e.vector
         FROM symbol_embeddings e
         JOIN symbols s ON s.id = e.symbol_id
         JOIN files f ON f.id = s.file_id
         WHERE e.model = ?1",
    );
    let mut bound: BoundValues = vec![Box::new(model.to_string())];
    push_scope(&mut sql, &mut bound, scope);

    let mut stmt = conn.prepare_cached(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let mut scored = Vec::new();
    let mut rows = stmt.query(params.as_slice())?;
    while let Some(row) = rows.next()? {
        let blob: Vec<u8> = row.get(9)?;
        let Some(similarity) = dot_encoded(&query, &blob) else {
            continue;
        };
        scored.push((
            similarity,
            SymbolHit {
                name: row.get(0)?,
                kind: row.get(1)?,
                language: row.get(2)?,
                relative_path: row.get(3)?,
                line: row.get(4)?,
                column: row.get(5)?,
                parent: row.get(6)?,
                end_line: row.get(7)?,
                level: row.get(8)?,
            },
        ));
    }
    scored.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.relative_path.cmp(&b.1.relative_path))
            .then_with(|| a.1.line.cmp(&b.1.line))
    });
    scored.truncate(limit);
    Ok(scored)
}

/// Lexical + semantic search fused with weighted Reciprocal Rank Fusion:
/// `score = (1 - alpha) / (RRF_K + lexical_rank) + alpha / (RRF_K + semantic_rank)`.
///
/// `alpha` is clamped to `0..=1` (NaN → 0). With `alpha == 0`, or no
/// `semantic` input (no embedder / no vectors), the result is exactly the
/// [`search_symbols`] ranking. Whatever `alpha`, a symbol whose name equals
/// the query (case-insensitively) ranks first, as it does lexically.
pub fn hybrid_search(
    conn: &Connection,
    query: &str,
    semantic: Option<(&[f32], &str)>,
    alpha: f64,
    scope: ResolvedScope<'_>,
) -> Result<Vec<HybridHit>> {
    let alpha = if alpha.is_nan() {
        0.0
    } else {
        alpha.clamp(0.0, 1.0)
    };
    let lexical = search_symbols(conn, query, scope)?;
    let semantic = match semantic {
        Some((vector, model)) if alpha > 0.0 => {
            semantic_ranking(conn, vector, model, scope, SEMANTIC_CANDIDATES)?
        }
        _ => Vec::new(),
    };
    let alpha = if semantic.is_empty() { 0.0 } else { alpha };

    type Key = (String, u32, u32, String);
    fn key(hit: &SymbolHit) -> Key {
        (
            hit.relative_path.clone(),
            hit.line,
            hit.column,
            hit.name.clone(),
        )
    }

    let mut fused: Vec<HybridHit> = Vec::new();
    let mut position: HashMap<Key, usize> = HashMap::new();
    for (i, hit) in lexical.into_iter().enumerate() {
        let rank = i + 1;
        position.insert(key(&hit), fused.len());
        fused.push(HybridHit {
            score: (1.0 - alpha) / (RRF_K + rank as f64),
            hit,
            lexical_rank: Some(rank),
            semantic_rank: None,
        });
    }
    for (i, (_, hit)) in semantic.into_iter().enumerate() {
        let rank = i + 1;
        let contribution = alpha / (RRF_K + rank as f64);
        match position.get(&key(&hit)) {
            Some(&at) => {
                if let Some(entry) = fused.get_mut(at) {
                    entry.score += contribution;
                    entry.semantic_rank = Some(rank);
                }
            }
            None => fused.push(HybridHit {
                hit,
                score: contribution,
                lexical_rank: None,
                semantic_rank: Some(rank),
            }),
        }
    }

    let query_lower = query.trim().to_lowercase();
    let is_exact = |h: &HybridHit| h.hit.name.to_lowercase() == query_lower;
    fused.retain(|h| h.score > 0.0 || is_exact(h));
    fused.sort_by(|a, b| {
        is_exact(b)
            .cmp(&is_exact(a))
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| a.hit.relative_path.cmp(&b.hit.relative_path))
            .then_with(|| a.hit.line.cmp(&b.hit.line))
    });
    Ok(fused)
}

fn normalized(mut vector: Vec<f32>) -> Vec<f32> {
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 && norm.is_finite() {
        vector.iter_mut().for_each(|x| *x /= norm);
    }
    vector
}

fn encode(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// Dot product of `query` with an encoded vector, or `None` when the
/// dimensions differ (a corrupt row, or a model change mid-flight).
fn dot_encoded(query: &[f32], blob: &[u8]) -> Option<f32> {
    if blob.len() != query.len() * 4 {
        return None;
    }
    Some(
        blob.as_chunks::<4>()
            .0
            .iter()
            .zip(query)
            .map(|(bytes, q)| f32::from_le_bytes(*bytes) * q)
            .sum(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_text_uses_split_words_and_parent() {
        assert_eq!(
            embedding_text(
                "parseRequestBody",
                "method",
                Some("HTTPParser"),
                "src/http_server.rs"
            ),
            "method parse request body in http parser (http server)"
        );
        assert_eq!(
            embedding_text("run", "function", None, "main.go"),
            "function run (main)"
        );
        assert_eq!(
            embedding_text("run", "function", Some(""), ""),
            "function run"
        );
    }

    #[test]
    fn encoded_vectors_round_trip_through_the_dot_product() {
        let v = normalized(vec![3.0, 4.0]);
        let blob = encode(&v);
        let dot = dot_encoded(&v, &blob).unwrap_or_default();
        assert!((dot - 1.0).abs() < 1e-6);
        assert_eq!(dot_encoded(&[1.0, 0.0, 0.0], &blob), None);
    }

    #[test]
    fn zero_vector_stays_finite() {
        assert_eq!(normalized(vec![0.0, 0.0]), vec![0.0, 0.0]);
    }
}
