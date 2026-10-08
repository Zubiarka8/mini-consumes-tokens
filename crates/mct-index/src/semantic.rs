//! Semantic half of hybrid search (Phase 2 of #31): per-symbol embedding
//! vectors, a brute-force cosine ranking over them, and Reciprocal Rank
//! Fusion with the lexical [`crate::search::search_symbols`] ranking.
//! Phase 3 (#63) enriched the embedded text with signature, doc comment and
//! callees ([`embedding_text`]) and added query-intent routing of `alpha`
//! ([`classify_query`]).
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

use std::collections::{BinaryHeap, HashMap};
use std::path::Path;

use rusqlite::Connection;

use crate::queries::{push_scope, symbol_hit, BoundValues, ResolvedScope, SymbolHit};
use crate::search::{
    exact_phrase, phrase_tier, qualified_parts, search_phrase, search_symbols_with,
    split_identifier, LexicalOptions,
};
use crate::{IndexError, Result};

/// RRF's rank-damping constant. Lower than the paper's usual 60: the
/// lexical list for a plain-language query is long and noisy, and a small
/// `k` lets each list's top few hits count for clearly more than its tail.
/// Tuned on `mct-mcp-server/tests/hybrid_benchmark.rs` (60, 20, 10, 5 tried).
pub const RRF_K: f64 = 10.0;

/// How many nearest symbols the semantic side contributes to the fusion.
/// Hits past this rank would add at most `alpha / (RRF_K + 200)` — noise.
pub const SEMANTIC_CANDIDATES: usize = 200;

/// Added to the fused score of an exact-phrase query's hits whose name
/// matches the phrase literally ([`phrase_tier`] 0..=2). Larger than any RRF
/// score (at most `1 / (RRF_K + 1)`), so those hits always outrank the rest.
pub const EXACT_PHRASE_BOOST: f64 = 1.0;

/// Symbols embedded per [`Embedder::embed`] call during
/// [`refresh_embeddings`], and per committed transaction.
///
/// Kept small on purpose: the local model pads every batch to its longest
/// text, so process RSS grows with the batch size (measured on this repo:
/// ~2.1 GB at 256, ~0.6 GB at 32; see `benchmarks/semantic-runtime.md`).
const EMBED_BATCH: usize = 32;

/// Turns text into vectors. Implemented outside this crate (see
/// `mct-mcp-server`'s `semantic` feature); tests use a deterministic fake.
pub trait Embedder: Send + Sync {
    /// Stable identifier of the model; stored per vector so switching models
    /// re-embeds instead of mixing incompatible vector spaces.
    fn model_id(&self) -> &str;

    /// One vector per input text, all of the same dimension, in input order.
    fn embed(&self, texts: &[String]) -> std::result::Result<Vec<Vec<f32>>, String>;

    /// The vector for a search query. Defaults to [`Embedder::embed`];
    /// asymmetric retrieval models (BGE, Arctic) override it to prepend the
    /// query instruction they were trained with.
    fn embed_query(&self, query: &str) -> std::result::Result<Vec<f32>, String> {
        self.embed(&[query.to_string()])?
            .pop()
            .ok_or_else(|| "embedder returned no vector for the query".to_string())
    }
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

/// Version of the [`embedding_text`] format. Part of the stored `model` tag
/// (see [`vector_space`]), so changing how the text is synthesised re-embeds
/// every symbol instead of mixing vectors of two different formats.
pub const EMBEDDING_TEXT_VERSION: &str = "ctx2";

/// Caps on the synthesised text, so one symbol stays far below the model's
/// token window (256 word pieces for MiniLM-class models) and a batch pads
/// to a short length — embedding cost grows with the longest text in it.
const MAX_SIGNATURE_CHARS: usize = 160;
const MAX_DOC_CHARS: usize = 240;
const MAX_CALLS: usize = 8;
const MAX_EMBEDDING_CHARS: usize = 640;

/// Files larger than this are not read for signature/doc context.
const MAX_CONTEXT_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// File stems that name a crate/package entry point rather than a topic;
/// the enclosing directory says more (`mct-index/src/lib.rs` → `mct index`).
const ENTRY_POINT_STEMS: &[&str] = &["lib", "mod", "main", "index", "__init__", "init"];

/// Everything one symbol's embedded text is synthesised from.
#[derive(Debug, Clone, Copy, Default)]
pub struct SymbolContext<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub language: &'a str,
    pub parent: Option<&'a str>,
    pub relative_path: &'a str,
    /// The declaration as written (up to its body), from the source file.
    pub signature: Option<&'a str>,
    /// The comment block directly above the declaration (or a Python-style
    /// docstring right below it), markers stripped.
    pub doc: Option<&'a str>,
    /// Names this symbol calls, from the `relations` table.
    pub calls: &'a [String],
}

/// The text a symbol is embedded from:
/// `[language: rust] [scope: exclude::exclude set] [symbol: method is excluded]
/// [signature: pub fn is_excluded(&self, path: &Path) -> bool] [doc: ...] [calls: ...]`.
/// Names are split into words, because sentence-embedding models tokenise
/// `parseRequestBody` into fragments that carry little meaning; the module
/// (file stem, or the directory for an entry-point file) says what area a
/// terse name like `parse_go_mod` belongs to; the doc comment carries the
/// plain-language intent a query describes. Empty sections are omitted and
/// the whole text is capped at [`MAX_EMBEDDING_CHARS`].
pub fn embedding_text(ctx: &SymbolContext<'_>) -> String {
    let words = |s: &str| split_identifier(s).join(" ");
    let mut sections = Vec::new();
    if !ctx.language.is_empty() {
        sections.push(format!("[language: {}]", ctx.language));
    }
    let module = module_words(ctx.relative_path);
    let parent = ctx.parent.map(words).filter(|p| !p.is_empty());
    match (module.is_empty(), parent) {
        (false, Some(parent)) => sections.push(format!("[scope: {module}::{parent}]")),
        (false, None) => sections.push(format!("[scope: {module}]")),
        (true, Some(parent)) => sections.push(format!("[scope: {parent}]")),
        (true, None) => {}
    }
    sections.push(format!("[symbol: {} {}]", ctx.kind, words(ctx.name)).replace("  ", " "));
    if let Some(sig) = ctx.signature.map(str::trim).filter(|s| !s.is_empty()) {
        sections.push(format!(
            "[signature: {}]",
            truncate_chars(sig, MAX_SIGNATURE_CHARS)
        ));
    }
    if let Some(doc) = ctx.doc.map(str::trim).filter(|d| !d.is_empty()) {
        sections.push(format!("[doc: {}]", truncate_chars(doc, MAX_DOC_CHARS)));
    }
    let calls: Vec<String> = ctx
        .calls
        .iter()
        .map(|c| words(c))
        .filter(|c| !c.is_empty())
        .take(MAX_CALLS)
        .collect();
    if !calls.is_empty() {
        sections.push(format!("[calls: {}]", calls.join(", ")));
    }
    truncate_chars(&sections.join(" "), MAX_EMBEDDING_CHARS).to_string()
}

/// The module a file stands for, as words: its stem, or for an entry-point
/// file (`lib.rs`, `__init__.py`) the nearest meaningful directory.
fn module_words(relative_path: &str) -> String {
    let mut segments = relative_path.rsplit('/');
    let stem = segments
        .next()
        .and_then(|file| file.split('.').next())
        .unwrap_or_default();
    let module = if ENTRY_POINT_STEMS.contains(&stem) {
        segments
            .find(|dir| !matches!(*dir, "src" | "lib" | "source" | "sources"))
            .unwrap_or(stem)
    } else {
        stem
    };
    split_identifier(module).join(" ")
}

/// `s` cut to at most `max` characters, on a char boundary.
fn truncate_chars(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((at, _)) => &s[..at],
        None => s,
    }
}

/// The declaration starting at 1-based `line`: that line and, while no body
/// opener has appeared yet, up to two more (multi-line parameter lists),
/// joined and cut before the body.
fn signature_at(lines: &[&str], line: u32, end_line: Option<u32>) -> Option<String> {
    let start = usize::try_from(line).ok()?.checked_sub(1)?;
    let last = end_line
        .and_then(|e| usize::try_from(e).ok())
        .map_or(start + 2, |e| e.saturating_sub(1).min(start + 2));
    let mut sig = String::new();
    for text in lines.get(start..=last.max(start).min(lines.len().saturating_sub(1)))? {
        let text = text.trim();
        let (head, done) = match text.find('{') {
            Some(at) => (&text[..at], true),
            None => (text, text.ends_with(':') || text.ends_with(';')),
        };
        if !sig.is_empty() {
            sig.push(' ');
        }
        sig.push_str(head.trim_end());
        if done {
            break;
        }
    }
    let sig = sig.trim().trim_end_matches(':').trim_end().to_string();
    (!sig.is_empty()).then_some(sig)
}

/// Comment markers stripped from a doc line, longest first.
const COMMENT_MARKERS: &[&str] = &["///", "//!", "//", "/**", "/*", "*/", "*", "--", "#"];

/// The comment block directly above 1-based `line` (skipping attribute and
/// decorator lines), or failing that a `"""`/`'''` docstring right below the
/// declaration — markers stripped, joined into one line.
fn doc_at(lines: &[&str], line: u32) -> Option<String> {
    let decl = usize::try_from(line).ok()?.checked_sub(1)?;
    let mut above = Vec::new();
    for text in lines.get(..decl)?.iter().rev() {
        let text = text.trim();
        if text.starts_with("#[") || text.starts_with('@') {
            continue;
        }
        let Some(marker) = COMMENT_MARKERS
            .iter()
            .find(|m| text.starts_with(**m) && !text.starts_with("#!"))
        else {
            break;
        };
        above.push(text[marker.len()..].trim_end_matches("*/").trim());
    }
    above.reverse();
    let mut doc = above.join(" ");
    if doc.trim().is_empty() {
        doc = docstring_below(lines, decl).unwrap_or_default();
    }
    let doc = doc.split_whitespace().collect::<Vec<_>>().join(" ");
    (!doc.is_empty()).then_some(doc)
}

fn docstring_below(lines: &[&str], decl: usize) -> Option<String> {
    let first = lines.get(decl + 1)?.trim();
    let quote = ["\"\"\"", "'''"]
        .into_iter()
        .find(|q| first.starts_with(q))?;
    let mut doc = Vec::new();
    for (i, text) in lines.get(decl + 1..)?.iter().enumerate().take(12) {
        let text = text.trim();
        let body = if i == 0 { &text[quote.len()..] } else { text };
        match body.find(quote) {
            Some(end) => {
                doc.push(&body[..end]);
                break;
            }
            None => doc.push(body),
        }
    }
    Some(doc.join(" "))
}

/// The lines of `relative_path` under `root`, or `None` when it is missing,
/// too large, not UTF-8, or resolves outside `root` (the same traversal
/// guard the reindex walk applies).
fn read_context_source(root: &Path, relative_path: &str) -> Option<String> {
    let canonical = root.join(relative_path).canonicalize().ok()?;
    if !canonical.starts_with(root) {
        return None;
    }
    if std::fs::metadata(&canonical).ok()?.len() > MAX_CONTEXT_FILE_BYTES {
        return None;
    }
    std::fs::read_to_string(&canonical).ok()
}

/// The `model` tag vectors are stored under: the embedder's model id plus
/// the [`embedding_text`] format version.
fn vector_space(model: &str) -> String {
    format!("{model}#{EMBEDDING_TEXT_VERSION}")
}

/// Embeds every symbol that has no vector for `embedder`'s model yet (new or
/// rewritten since the last call), and drops vectors from any other model
/// or text format. Returns how many symbols were embedded. Incremental:
/// after the first full pass, a call only pays for what the last reindex
/// changed. Signature and doc context are read from the files under `root`,
/// each file at most once per call.
///
/// This is [`snapshot_pending_embeddings`] → [`PendingEmbeddings::embed_batch`]
/// → [`commit_embedding_batch`] run back to back on one connection. A caller
/// that must not hold the connection while the model runs drives the three
/// steps itself, taking the connection only for the first and last.
pub fn refresh_embeddings(
    conn: &Connection,
    root: &Path,
    embedder: &dyn Embedder,
) -> Result<usize> {
    let pending = snapshot_pending_embeddings(conn, root, embedder.model_id())?;
    let mut committed = 0;
    for batch in 0..pending.batch_count() {
        let vectors = pending.embed_batch(batch, embedder)?;
        committed += commit_embedding_batch(conn, &pending, batch, vectors)?;
    }
    Ok(committed)
}

/// The symbols that had no vector at snapshot time, with the exact text to
/// embed for each and the identity (see [`commit_embedding_batch`]) a vector
/// must still match to be stored. Holds no connection: embedding it
/// ([`PendingEmbeddings::embed_batch`]) needs nothing but the model.
#[derive(Debug)]
pub struct PendingEmbeddings {
    space: String,
    items: Vec<PendingItem>,
}

#[derive(Debug)]
struct PendingItem {
    id: i64,
    name: String,
    kind: String,
    parent: Option<String>,
    path: String,
    line: u32,
    end_line: Option<u32>,
    file_hash: String,
    text: String,
}

impl PendingEmbeddings {
    /// How many symbols are pending.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// How many [`EMBED_BATCH`]-sized batches the snapshot splits into.
    pub fn batch_count(&self) -> usize {
        self.items.len().div_ceil(EMBED_BATCH)
    }

    fn batch(&self, batch: usize) -> &[PendingItem] {
        let start = batch.saturating_mul(EMBED_BATCH).min(self.items.len());
        let end = start.saturating_add(EMBED_BATCH).min(self.items.len());
        &self.items[start..end]
    }

    /// One vector per symbol of `batch`, in order. Errors if `embedder` is
    /// not the model this snapshot was taken for (its vectors would land in
    /// the wrong space) or returns the wrong number of vectors.
    pub fn embed_batch(&self, batch: usize, embedder: &dyn Embedder) -> Result<Vec<Vec<f32>>> {
        if vector_space(embedder.model_id()) != self.space {
            return Err(IndexError::Embedding(format!(
                "snapshot is for `{}`, not `{}`",
                self.space,
                embedder.model_id()
            )));
        }
        let items = self.batch(batch);
        let texts: Vec<String> = items.iter().map(|item| item.text.clone()).collect();
        let vectors = embedder.embed(&texts).map_err(IndexError::Embedding)?;
        if vectors.len() != items.len() {
            return Err(IndexError::Embedding(format!(
                "embedder returned {} vectors for {} texts",
                vectors.len(),
                items.len()
            )));
        }
        Ok(vectors)
    }
}

/// Step 1 of a refresh: drops vectors of any other model or text format, and
/// collects what still needs a vector for `model_id`, reading signature and
/// doc context from the files under `root` (each file once). Cheap next to
/// embedding; the only step besides the commit that touches `conn`.
pub fn snapshot_pending_embeddings(
    conn: &Connection,
    root: &Path,
    model_id: &str,
) -> Result<PendingEmbeddings> {
    let space = vector_space(model_id);
    conn.execute("DELETE FROM symbol_embeddings WHERE model <> ?1", [&space])?;

    struct Pending {
        id: i64,
        name: String,
        kind: String,
        language: String,
        parent: Option<String>,
        path: String,
        line: u32,
        end_line: Option<u32>,
        has_level: bool,
        file_hash: String,
        calls: Vec<String>,
    }
    let rows: Vec<Pending> = {
        let mut stmt = conn.prepare_cached(
            "SELECT s.id, s.name, s.kind, f.language, s.parent, f.relative_path, s.line,
                    s.end_line, s.level IS NOT NULL, f.content_hash,
                    (SELECT group_concat(to_name, ' ') FROM (
                        SELECT DISTINCT r.to_name FROM relations r
                        WHERE r.from_symbol_id = s.id AND r.kind = 'calls'
                        ORDER BY r.line LIMIT ?1))
             FROM symbols s
             JOIN files f ON f.id = s.file_id
             LEFT JOIN symbol_embeddings e ON e.symbol_id = s.id
             WHERE e.symbol_id IS NULL
             ORDER BY f.relative_path, s.line",
        )?;
        let rows = stmt.query_map([MAX_CALLS as i64], |row| {
            let calls: Option<String> = row.get(10)?;
            Ok(Pending {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                language: row.get(3)?,
                parent: row.get(4)?,
                path: row.get(5)?,
                line: row.get(6)?,
                end_line: row.get(7)?,
                has_level: row.get(8)?,
                file_hash: row.get(9)?,
                calls: calls
                    .unwrap_or_default()
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
            })
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };

    // Rows are ordered by path: each file is read and split once.
    let mut items: Vec<PendingItem> = Vec::with_capacity(rows.len());
    for file_rows in rows.chunk_by(|a, b| a.path == b.path) {
        let source = file_rows
            .first()
            .and_then(|p| read_context_source(root, &p.path))
            .unwrap_or_default();
        let lines: Vec<&str> = source.lines().collect();
        for p in file_rows {
            let signature = signature_at(&lines, p.line, p.end_line);
            // A heading-like symbol (Markdown) has no doc comment above it.
            let doc = if p.has_level {
                None
            } else {
                doc_at(&lines, p.line)
            };
            let text = embedding_text(&SymbolContext {
                name: &p.name,
                kind: &p.kind,
                language: &p.language,
                parent: p.parent.as_deref(),
                relative_path: &p.path,
                signature: signature.as_deref(),
                doc: doc.as_deref(),
                calls: &p.calls,
            });
            items.push(PendingItem {
                id: p.id,
                name: p.name.clone(),
                kind: p.kind.clone(),
                parent: p.parent.clone(),
                path: p.path.clone(),
                line: p.line,
                end_line: p.end_line,
                file_hash: p.file_hash.clone(),
                text,
            });
        }
    }
    // Batches of similar length: a batch is padded to its longest text, so
    // mixing a 20-token name with a 150-token doc wastes most of the work.
    // Stable, so equal lengths keep their (path, line) order.
    items.sort_by_key(|item| item.text.len());
    Ok(PendingEmbeddings { space, items })
}

/// Step 3 of a refresh: stores `vectors` (from [`PendingEmbeddings::embed_batch`]
/// for the same `batch`) in one transaction, and returns how many were stored.
///
/// A vector is stored only if its symbol still *is* the one that was embedded:
/// same id, name, kind, parent, line, end line and file path, in a file whose
/// content hash is unchanged. `symbols.id` is a bare rowid that SQLite may
/// reuse after a delete, so the id alone does not identify a symbol; a symbol
/// deleted or rewritten by a reindex between snapshot and commit is skipped,
/// never resurrected with a vector computed from its old text. Skipped
/// symbols simply stay pending for the next snapshot.
pub fn commit_embedding_batch(
    conn: &Connection,
    pending: &PendingEmbeddings,
    batch: usize,
    vectors: Vec<Vec<f32>>,
) -> Result<usize> {
    let items = pending.batch(batch);
    if vectors.len() != items.len() {
        return Err(IndexError::Embedding(format!(
            "{} vectors for {} symbols",
            vectors.len(),
            items.len()
        )));
    }
    let mut stored = 0;
    let tx = conn.unchecked_transaction()?;
    {
        let mut insert = tx.prepare_cached(
            "INSERT OR REPLACE INTO symbol_embeddings(symbol_id, model, vector)
             SELECT s.id, ?2, ?3
             FROM symbols s JOIN files f ON f.id = s.file_id
             WHERE s.id = ?1 AND s.name = ?4 AND s.kind = ?5 AND s.parent IS ?6
               AND s.line = ?7 AND s.end_line IS ?8
               AND f.relative_path = ?9 AND f.content_hash = ?10",
        )?;
        for (item, vector) in items.iter().zip(vectors) {
            stored += insert.execute(rusqlite::params![
                item.id,
                pending.space,
                encode(&normalized(vector)),
                item.name,
                item.kind,
                item.parent,
                item.line,
                item.end_line,
                item.path,
                item.file_hash,
            ])?;
        }
    }
    tx.commit()?;
    Ok(stored)
}

/// Embedded vs. total symbol counts for `model`.
pub fn embedding_coverage(conn: &Connection, model: &str) -> Result<EmbeddingCoverage> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))?;
    let embedded: i64 = conn.query_row(
        "SELECT COUNT(*) FROM symbol_embeddings WHERE model = ?1",
        [vector_space(model)],
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
    if limit == 0 {
        return Ok(Vec::new());
    }
    let query = normalized(query_vector.to_vec());
    let mut sql = String::from(
        "SELECT s.name, s.kind, f.language, f.relative_path, s.line, s.column, s.parent, s.end_line, s.level, s.id,
                e.vector
         FROM symbol_embeddings e
         JOIN symbols s ON s.id = e.symbol_id
         JOIN files f ON f.id = s.file_id
         WHERE e.model = ?1",
    );
    let mut bound: BoundValues = vec![Box::new(vector_space(model))];
    push_scope(&mut sql, &mut bound, scope);

    let mut stmt = conn.prepare_cached(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    // Bounded top-k: a max-heap of the `limit` best so far (its top is the
    // worst kept), so memory is O(limit) rather than one `SymbolHit` per
    // embedded row. `Ranked`'s order is the old full sort's (similarity,
    // path, line) plus the scan position, i.e. exactly what a stable sort
    // did with ties — the result is identical, not merely equivalent.
    let mut best: BinaryHeap<Ranked> = BinaryHeap::with_capacity(limit.min(1024) + 1);
    let mut rows = stmt.query(params.as_slice())?;
    let mut seq = 0usize;
    while let Some(row) = rows.next()? {
        let blob: Vec<u8> = row.get(10)?;
        let Some(similarity) = dot_encoded(&query, &blob) else {
            continue;
        };
        seq += 1;
        // A strictly lower similarity than the worst kept can never enter:
        // skip it before paying to build its hit.
        if best.len() >= limit
            && best
                .peek()
                .is_some_and(|worst| similarity.total_cmp(&worst.similarity).is_lt())
        {
            continue;
        }
        let candidate = Ranked {
            similarity,
            seq,
            hit: symbol_hit(row)?,
        };
        if best.len() < limit {
            best.push(candidate);
        } else if best.peek().is_some_and(|worst| candidate < *worst) {
            best.pop();
            best.push(candidate);
        }
    }
    Ok(best
        .into_sorted_vec()
        .into_iter()
        .map(|r| (r.similarity, r.hit))
        .collect())
}

/// One scored row in [`semantic_ranking`]'s heap. `Ord` is ranking order:
/// `a < b` when `a` ranks ahead of `b` (higher similarity, then path, line,
/// and finally scan position, which makes the order total).
struct Ranked {
    similarity: f32,
    seq: usize,
    hit: SymbolHit,
}

impl Ord for Ranked {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .similarity
            .total_cmp(&self.similarity)
            .then_with(|| self.hit.relative_path.cmp(&other.hit.relative_path))
            .then_with(|| self.hit.line.cmp(&other.hit.line))
            .then_with(|| self.seq.cmp(&other.seq))
    }
}

impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Ranked {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Ranked {}

/// What a `hybrid_search` query looks like, as far as a few string checks
/// can tell — used to pick `alpha` when the caller doesn't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryIntent {
    /// Every word is code-shaped (`snake_case`, `camelCase`/`PascalCase`
    /// with an inner capital, `Type::member`, `module.function`): the user
    /// knows the name, so the lexical ranking should lead.
    Identifier,
    /// Several plain words, none code-shaped: the user describes behaviour,
    /// so the semantic ranking should lead.
    NaturalLanguage,
    /// Anything else — one plain word, or prose mixed with identifiers.
    Mixed,
    /// The whole query is `"double-quoted"`: a literal phrase, matched
    /// strictly and lexically only — see [`crate::exact_phrase`].
    ExactPhrase,
}

impl QueryIntent {
    /// The `alpha` this intent maps to.
    pub fn alpha(self) -> f64 {
        match self {
            QueryIntent::Identifier => 0.1,
            QueryIntent::NaturalLanguage => 0.75,
            QueryIntent::Mixed => 0.5,
            QueryIntent::ExactPhrase => 0.0,
        }
    }
}

/// Classifies `query` by its surface form only — no index lookup, a few
/// hundred nanoseconds. See [`QueryIntent`].
pub fn classify_query(query: &str) -> QueryIntent {
    if exact_phrase(query).is_some() {
        return QueryIntent::ExactPhrase;
    }
    let words: Vec<&str> = query.split_whitespace().collect();
    let code_shaped = words.iter().filter(|w| is_code_shaped(w)).count();
    match (words.len(), code_shaped) {
        (0, _) => QueryIntent::Mixed,
        (n, c) if c == n => QueryIntent::Identifier,
        (1, _) => QueryIntent::Mixed,
        (_, 0) => QueryIntent::NaturalLanguage,
        _ => QueryIntent::Mixed,
    }
}

/// Whether one whitespace-free word follows a code identifier convention.
fn is_code_shaped(word: &str) -> bool {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
    if word.contains("::") || word.contains("->") {
        return true;
    }
    let chars: Vec<char> = word.chars().collect();
    let ident = |c: &char| c.is_alphanumeric() || *c == '_';
    // An inner `_` between identifier characters: `a_b`, `parse_go_mod`.
    let snake = chars
        .windows(3)
        .any(|w| w[1] == '_' && w[0].is_alphanumeric() && ident(&w[2]));
    // Dotted identifier segments of 2+ chars: `toon.decode` (not `e.g`).
    let dotted = word.contains('.')
        && word
            .split('.')
            .all(|seg| seg.chars().count() >= 2 && seg.chars().all(|c| ident(&c)));
    // A lower→upper transition: `camelCase`, `PascalCase`, `parseHTTP`.
    let inner_capital = chars
        .windows(2)
        .any(|w| w[0].is_lowercase() && w[1].is_uppercase());
    snake || dotted || inner_capital
}

/// Lexical + semantic search fused with weighted Reciprocal Rank Fusion:
/// `score = (1 - alpha) / (RRF_K + lexical_rank) + alpha / (RRF_K + semantic_rank)`.
///
/// The lexical side is [`crate::search::search_symbols`] plus two
/// hybrid-only [`LexicalOptions`]: dev-verb synonyms (`get`/`fetch`/`read`,
/// ...) and qualified names (`Index::open` finds `open` inside `Index`
/// first). `alpha` is clamped to `0..=1` (NaN → 0). With `alpha == 0`, or no
/// `semantic` input (no embedder / no vectors), the result is exactly that
/// lexical ranking. Whatever `alpha`, a symbol whose name equals the query
/// (case-insensitively) ranks first, as it does lexically. Callers that
/// don't choose `alpha` themselves can take it from [`classify_query`].
///
/// A `"double-quoted"` query is an exact phrase ([`crate::exact_phrase`]):
/// `alpha` is forced to 0 (the semantic side is skipped), the lexical side
/// only matches names holding the phrase's words consecutively and in order,
/// and hits whose name matches the phrase literally get
/// [`EXACT_PHRASE_BOOST`], so the literal match ranks first.
pub fn hybrid_search(
    conn: &Connection,
    query: &str,
    semantic: Option<(&[f32], &str)>,
    alpha: f64,
    scope: ResolvedScope<'_>,
) -> Result<Vec<HybridHit>> {
    let phrase = exact_phrase(query);
    let alpha = if phrase.is_some() || alpha.is_nan() {
        0.0
    } else {
        alpha.clamp(0.0, 1.0)
    };
    let lexical = match phrase {
        Some(phrase) => search_phrase(conn, phrase, scope)?,
        None => {
            let options = LexicalOptions {
                synonyms: true,
                qualified: true,
            };
            search_symbols_with(conn, query, scope, options)?
        }
    };
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

    if let Some(phrase) = phrase {
        let literal = qualified_parts(phrase).map_or(phrase, |(_, last)| last);
        for entry in &mut fused {
            if phrase_tier(&entry.hit.name, literal) <= 2 {
                entry.score += EXACT_PHRASE_BOOST;
            }
        }
    }

    let query_lower = phrase.unwrap_or(query).trim().to_lowercase();
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
    fn embedding_text_synthesises_every_section() {
        let calls = ["readToEnd".to_string(), "splitLines".to_string()];
        assert_eq!(
            embedding_text(&SymbolContext {
                name: "parseRequestBody",
                kind: "method",
                language: "rust",
                parent: Some("HTTPParser"),
                relative_path: "src/http_server.rs",
                signature: Some("pub fn parse_request_body(&self) -> Body"),
                doc: Some("Reads the whole body."),
                calls: &calls,
            }),
            "[language: rust] [scope: http server::http parser] \
             [symbol: method parse request body] \
             [signature: pub fn parse_request_body(&self) -> Body] \
             [doc: Reads the whole body.] [calls: read to end, split lines]"
        );
    }

    #[test]
    fn embedding_text_omits_empty_sections() {
        assert_eq!(
            embedding_text(&SymbolContext {
                name: "run",
                kind: "function",
                parent: Some(""),
                ..SymbolContext::default()
            }),
            "[symbol: function run]"
        );
        // An entry-point file stands for its directory.
        assert_eq!(
            embedding_text(&SymbolContext {
                name: "Index",
                kind: "struct",
                language: "rust",
                relative_path: "crates/mct-index/src/lib.rs",
                ..SymbolContext::default()
            }),
            "[language: rust] [scope: mct index] [symbol: struct index]"
        );
    }

    #[test]
    fn embedding_text_is_capped() {
        let doc = "word ".repeat(500);
        let text = embedding_text(&SymbolContext {
            name: "f",
            kind: "function",
            doc: Some(&doc),
            ..SymbolContext::default()
        });
        assert!(text.chars().count() <= MAX_EMBEDDING_CHARS);
    }

    #[test]
    fn signature_stops_at_the_body() {
        let src = [
            "/// Doc.",
            "pub fn a(",
            "    x: u32,",
            ") -> u32 {",
            "    x",
            "}",
        ];
        assert_eq!(
            signature_at(&src, 2, Some(6)).as_deref(),
            Some("pub fn a( x: u32, ) -> u32")
        );
        assert_eq!(
            signature_at(&["def f(x):", "    return x"], 1, Some(2)).as_deref(),
            Some("def f(x)")
        );
        assert_eq!(signature_at(&src, 0, None), None);
        assert_eq!(signature_at(&src, 99, None), None);
    }

    #[test]
    fn doc_comes_from_the_comment_block_above_or_a_docstring_below() {
        let src = [
            "use x;",
            "/// Loads the",
            "/// settings.",
            "#[inline]",
            "fn load() {}",
        ];
        assert_eq!(doc_at(&src, 5).as_deref(), Some("Loads the settings."));
        assert_eq!(doc_at(&src, 1), None);
        let py = [
            "def f():",
            "    \"\"\"Return one.",
            "    Always.\"\"\"",
            "    return 1",
        ];
        assert_eq!(doc_at(&py, 1).as_deref(), Some("Return one. Always."));
        let block = ["/**", " * Draws it.", " */", "function draw() {}"];
        assert_eq!(doc_at(&block, 4).as_deref(), Some("Draws it."));
    }

    #[test]
    fn identifier_shaped_queries_route_to_lexical() {
        for q in [
            "read_ignore_file",
            "fanInCounts",
            "LanguageRegistry",
            "Index::hybrid_search",
            "toon.decode_table",
            "self->visit",
            "parse_go_mod walker_new",
        ] {
            assert_eq!(classify_query(q), QueryIntent::Identifier, "{q}");
        }
        assert_eq!(QueryIntent::Identifier.alpha(), 0.1);
    }

    #[test]
    fn quoted_queries_route_to_exact_phrase_with_no_semantic_weight() {
        for q in [
            "\"parse request\"",
            " \"Index::open\" ",
            "\"connection refused: db\"",
        ] {
            assert_eq!(classify_query(q), QueryIntent::ExactPhrase, "{q}");
        }
        assert_eq!(classify_query("\"unterminated"), QueryIntent::Mixed);
        assert_eq!(QueryIntent::ExactPhrase.alpha(), 0.0);
    }

    #[test]
    fn plain_prose_routes_to_semantic() {
        for q in [
            "load settings from disk",
            "directory tree",
            "is this a test file?",
        ] {
            assert_eq!(classify_query(q), QueryIntent::NaturalLanguage, "{q}");
        }
        assert_eq!(QueryIntent::NaturalLanguage.alpha(), 0.75);
    }

    #[test]
    fn single_plain_words_and_mixtures_stay_balanced() {
        for q in [
            "",
            "   ",
            "bfs",
            "Walker",
            "break camelCase names",
            "python Walker push_relation",
            "e.g.",
        ] {
            assert_eq!(classify_query(q), QueryIntent::Mixed, "{q:?}");
        }
        assert_eq!(QueryIntent::Mixed.alpha(), 0.5);
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

    // ---- ranking and refresh against the real schema ----------------------

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    const MODEL: &str = "test/m";

    fn index() -> std::result::Result<crate::Index, Box<dyn std::error::Error>> {
        Ok(crate::Index::open_in_memory(
            &std::env::temp_dir(),
            crate::ExcludeSet::default(),
        )?)
    }

    fn add_file(conn: &Connection, id: i64, path: &str, language: &str, hash: &str) -> TestResult {
        conn.execute(
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at)
             VALUES (?1, ?2, ?3, ?4, 0)",
            rusqlite::params![id, path, language, hash],
        )?;
        Ok(())
    }

    fn add_symbol(conn: &Connection, id: i64, file_id: i64, name: &str, line: u32) -> TestResult {
        conn.execute(
            "INSERT INTO symbols (id, file_id, name, kind, line, column, byte_len)
             VALUES (?1, ?2, ?3, 'function', ?4, 1, 1)",
            rusqlite::params![id, file_id, name, line],
        )?;
        Ok(())
    }

    fn add_vector(conn: &Connection, id: i64, vector: &[f32]) -> TestResult {
        conn.execute(
            "INSERT INTO symbol_embeddings (symbol_id, model, vector) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                id,
                vector_space(MODEL),
                encode(&normalized(vector.to_vec()))
            ],
        )?;
        Ok(())
    }

    /// Names of `semantic_ranking`'s answer, with the similarities rounded
    /// so equal-looking ties compare equal.
    fn ranked(
        conn: &Connection,
        scope: ResolvedScope<'_>,
        limit: usize,
    ) -> std::result::Result<Vec<(String, i32)>, Box<dyn std::error::Error>> {
        Ok(
            semantic_ranking(conn, &[2.0, 0.0, 0.0], MODEL, scope, limit)?
                .into_iter()
                .map(|(sim, hit)| (hit.name, (sim * 1000.0).round() as i32))
                .collect(),
        )
    }

    /// The bounded top-k answers exactly what a full sort then truncate would,
    /// for every limit: ties on similarity (broken by path, line, then scan
    /// order), zero and wrong-dimension vectors, limits 0, 1, k and k > rows.
    #[test]
    fn top_k_equals_the_full_sort_for_every_limit() -> TestResult {
        let index = index()?;
        let conn = &index.conn;
        add_file(conn, 1, "a.rs", "rust", "h")?;
        add_file(conn, 2, "b.rs", "rust", "h")?;
        add_file(conn, 3, "c.rs", "rust", "h")?;
        add_file(conn, 4, "d.py", "python", "h")?;
        // (id, file, name, line, vector). Query is the x axis.
        let rows: [(i64, i64, &str, u32, &[f32]); 10] = [
            (1, 1, "tie_a10", 10, &[1.0, 0.0, 0.0]),
            (2, 2, "tie_b5", 5, &[3.0, 0.0, 0.0]),
            (3, 1, "tie_a3", 3, &[1.0, 0.0, 0.0]),
            (4, 3, "mid", 1, &[0.6, 0.8, 0.0]),
            (5, 1, "orth_a20", 20, &[0.0, 1.0, 0.0]),
            (6, 2, "zero_b1", 1, &[0.0, 0.0, 0.0]),
            (7, 3, "wrong_dim", 9, &[1.0, 0.0]),
            (8, 3, "opposite", 2, &[-1.0, 0.0, 0.0]),
            // Same similarity, path and line as `tie_a10`: scan order decides.
            (9, 1, "tie_a10_twin", 10, &[1.0, 0.0, 0.0]),
            (10, 4, "python_best", 1, &[1.0, 0.0, 0.0]),
        ];
        for (id, file, name, line, vector) in rows {
            add_symbol(conn, id, file, name, line)?;
            add_vector(conn, id, vector)?;
        }

        let everything = ResolvedScope::default();
        let full = ranked(conn, everything, usize::MAX)?;
        assert_eq!(
            full.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            [
                "tie_a3",
                "tie_a10",
                "tie_a10_twin",
                "tie_b5",
                "python_best",
                "mid",
                "orth_a20",
                "zero_b1",
                "opposite"
            ]
        );
        assert_eq!(full.len(), 9, "the wrong-dimension row never ranks");
        for limit in [0, 1, 2, 3, 4, 8, 9, 10, 1000] {
            let got = ranked(conn, everything, limit)?;
            assert_eq!(got, full[..limit.min(full.len())], "limit {limit}");
        }

        let python = ResolvedScope {
            language: Some("python"),
            ..ResolvedScope::default()
        };
        assert_eq!(
            ranked(conn, python, 5)?,
            [("python_best".to_string(), 1000)]
        );
        let file_a = ResolvedScope {
            path: Some("a.rs"),
            path_is_file: true,
            language: None,
        };
        let in_a: Vec<String> = ranked(conn, file_a, 2)?
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(in_a, ["tie_a3", "tie_a10"]);
        Ok(())
    }

    struct Fake(&'static str);

    impl Embedder for Fake {
        fn model_id(&self) -> &str {
            self.0
        }
        fn embed(&self, texts: &[String]) -> std::result::Result<Vec<Vec<f32>>, String> {
            Ok(texts
                .iter()
                .map(|t| vec![t.len() as f32, 1.0, 0.0])
                .collect())
        }
    }

    fn embedded_ids(conn: &Connection) -> std::result::Result<Vec<i64>, rusqlite::Error> {
        conn.prepare("SELECT symbol_id FROM symbol_embeddings ORDER BY symbol_id")?
            .query_map([], |r| r.get(0))?
            .collect()
    }

    /// Snapshot → embed → commit never stores a vector for a symbol that a
    /// reindex deleted, rewrote or re-hashed in between, even when SQLite
    /// hands a deleted symbol's rowid to a different one.
    #[test]
    fn commit_skips_symbols_changed_since_the_snapshot() -> TestResult {
        let index = index()?;
        let conn = &index.conn;
        add_file(conn, 1, "a.rs", "rust", "h1")?;
        add_file(conn, 2, "b.rs", "rust", "h2")?;
        for (id, file, name) in [
            (1, 1, "deleted"),
            (2, 1, "reused_id"),
            (3, 1, "kept"),
            (4, 2, "rehashed"),
        ] {
            add_symbol(conn, id, file, name, id as u32)?;
        }
        let embedder = Fake(MODEL);
        let pending = snapshot_pending_embeddings(conn, &index.root, MODEL)?;
        assert_eq!(pending.len(), 4);
        assert_eq!(pending.batch_count(), 1);

        // What a reindex between snapshot and commit can do.
        conn.execute("DELETE FROM symbols WHERE id IN (1, 2)", [])?;
        add_symbol(conn, 2, 1, "someone_else", 2)?;
        conn.execute("UPDATE files SET content_hash = 'h2-new' WHERE id = 2", [])?;

        let vectors = pending.embed_batch(0, &embedder)?;
        assert_eq!(commit_embedding_batch(conn, &pending, 0, vectors)?, 1);
        assert_eq!(embedded_ids(conn)?, [3]);

        // The skipped ones are still pending, not lost: the next snapshot
        // picks up the new symbol and the re-hashed file's.
        let again = snapshot_pending_embeddings(conn, &index.root, MODEL)?;
        assert_eq!(again.len(), 2);
        Ok(())
    }

    /// Memory guard: the embedding batch size bounds the model's working set.
    #[test]
    fn pending_embeddings_split_into_small_batches() -> TestResult {
        let index = index()?;
        let conn = &index.conn;
        add_file(conn, 1, "a.rs", "rust", "h")?;
        for id in 1..=70 {
            add_symbol(conn, id, 1, &format!("sym{id}"), id as u32)?;
        }
        let pending = snapshot_pending_embeddings(conn, &index.root, MODEL)?;
        assert_eq!(pending.len(), 70);
        assert_eq!(pending.batch_count(), 3, "70 symbols at 32 per batch");
        assert_eq!(pending.embed_batch(0, &Fake(MODEL))?.len(), 32);
        Ok(())
    }

    #[test]
    fn refresh_is_incremental_and_rejects_a_foreign_model() -> TestResult {
        let index = index()?;
        let conn = &index.conn;
        add_file(conn, 1, "a.rs", "rust", "h")?;
        add_symbol(conn, 1, 1, "one", 1)?;
        add_symbol(conn, 2, 1, "two", 2)?;
        assert_eq!(refresh_embeddings(conn, &index.root, &Fake(MODEL))?, 2);
        assert_eq!(refresh_embeddings(conn, &index.root, &Fake(MODEL))?, 0);

        // A snapshot is for one vector space: another model can't fill it.
        add_symbol(conn, 3, 1, "three", 3)?;
        let pending = snapshot_pending_embeddings(conn, &index.root, MODEL)?;
        assert_eq!(pending.len(), 1);
        assert!(pending.embed_batch(0, &Fake("other/m")).is_err());
        // Out-of-range batches are empty, not a panic.
        assert!(pending.embed_batch(7, &Fake(MODEL))?.is_empty());
        Ok(())
    }
}
