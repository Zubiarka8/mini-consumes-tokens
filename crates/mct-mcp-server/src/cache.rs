//! Query cache for the ranked search tools (`search_symbols`, `hybrid_search`)
//! — issue #16.
//!
//! Two lookup paths, both in front of the same validation:
//!
//! - **Exact**: a SHA-256 of the repository identity, the tool and its cache
//!   version, the normalised query, every parameter the *ranking* depends on
//!   (scope, `alpha`), the search configuration and the embedding model.
//!   A `HashMap` probe.
//! - **Semantic** (`hybrid_search` with a semantic side only): the query's
//!   embedding — the one the search needs anyway — is compared by cosine
//!   similarity against earlier queries cached under the same key minus the
//!   query text. A similarity at or above the threshold (0.95 by default)
//!   only makes a *candidate*: it must also be unambiguous (clear of the
//!   runner-up by [`CacheConfig::min_margin`]) and pass validation.
//!
//! What is cached is the **ranking** — the ordered symbols a query resolved
//! to — never rendered text or source code. A hit is rendered afresh from the
//! current index and the current files on disk, exactly like a miss, so it
//! cannot show stale code. Validation decides whether that ranking is still
//! the one the current index would produce:
//!
//! 1. Repository state unchanged (SHA-256 over every indexed file's content
//!    hash) — the ranking is a pure function of the index and the key.
//! 2. Every ranked symbol still exists (same path, name, kind, line, column),
//!    else `stale_rejection`.
//! 3. The result fingerprint — every ranked symbol's full row plus the
//!    content hash of each file holding one — still matches, else
//!    `fingerprint_mismatch`. So any edit to a file a result points into
//!    invalidates it (granular invalidation by referenced file).
//! 4. If the repository state moved but the referenced files didn't (an
//!    unrelated change), the ranking is recomputed from the stored query
//!    (and stored vector — no re-embedding) and compared: identical keeps the
//!    entry, re-stamped with the new state; different is a
//!    `repository_mismatch` miss. Lexical BM25 statistics and the semantic
//!    top-N are corpus-wide, so an unrelated file *can* change a ranking —
//!    this re-check is what lets unrelated changes keep the cache without
//!    ever trusting it blindly.
//!
//! Anything that fails, errors or is ambiguous is a miss: a false hit is
//! worse than a miss.
//!
//! Token saving: when a hit renders byte-for-byte the response this session
//! already returned, the reply is a one-line "unchanged" note instead of the
//! full text ([`ResponseMode::Reference`]). The tool's `cache: false` argument
//! (or `MCT_CACHE_RESPONSE=full`) always gets the full text.
//!
//! The cache lives in the server process — one MCP session — and is never
//! written to disk: no schema migration, no stale state across restarts.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::{Duration, Instant};

use mct_index::{Index, SymbolHit};

/// Master switch: `off`/`0`/`false` disables every cache path, restoring the
/// uncached behaviour exactly.
pub const CACHE_ENV: &str = "MCT_CACHE";
/// `off` keeps the exact cache but disables the semantic one.
pub const SEMANTIC_CACHE_ENV: &str = "MCT_SEMANTIC_CACHE";
/// Cosine similarity a semantic candidate needs, `0.5..=1.0`.
pub const THRESHOLD_ENV: &str = "MCT_CACHE_THRESHOLD";
/// Minimum gap between the best and second-best semantic candidate.
pub const MARGIN_ENV: &str = "MCT_CACHE_MARGIN";
/// `reference` (default) or `full` — what a hit already returned in this
/// session replies with.
pub const RESPONSE_ENV: &str = "MCT_CACHE_RESPONSE";
/// Entries kept before the least recently used is evicted.
pub const MAX_ENTRIES_ENV: &str = "MCT_CACHE_MAX_ENTRIES";

pub const DEFAULT_THRESHOLD: f32 = 0.95;
pub const DEFAULT_MIN_MARGIN: f32 = 0.02;
pub const DEFAULT_MAX_ENTRIES: usize = 256;
/// Lowest threshold accepted from the environment: below it "similar"
/// stops meaning "same question".
const MIN_THRESHOLD: f32 = 0.5;

/// Digests of responses already returned this session, kept for the
/// reference reply; oldest dropped past this.
const MAX_SENT: usize = 1024;

/// Bumped whenever what a cache entry means changes shape.
pub const CACHE_FORMAT_VERSION: &str = "mct-query-cache/1";

pub type Digest = [u8; 32];

fn sha256(bytes: &[u8]) -> Digest {
    hmac_sha256::Hash::hash(bytes)
}

/// Feeds length-prefixed fields to a SHA-256, so `("ab", "c")` and
/// `("a", "bc")` can never collide.
struct Hasher(hmac_sha256::Hash);

impl Hasher {
    fn new() -> Self {
        Self(hmac_sha256::Hash::new())
    }
    fn field(&mut self, bytes: impl AsRef<[u8]>) -> &mut Self {
        let bytes = bytes.as_ref();
        self.0.update((bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
        self
    }
    fn opt(&mut self, value: Option<&str>) -> &mut Self {
        match value {
            Some(v) => self.field([1]).field(v),
            None => self.field([0]),
        }
    }
    fn finish(self) -> Digest {
        self.0.finalize()
    }
}

/// What a hit already returned in this session replies with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseMode {
    /// A one-line "unchanged" note — the token saving.
    Reference,
    /// The full re-rendered response, every time.
    Full,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CacheConfig {
    pub enabled: bool,
    pub semantic: bool,
    pub threshold: f32,
    pub min_margin: f32,
    pub response: ResponseMode,
    pub max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            semantic: true,
            threshold: DEFAULT_THRESHOLD,
            min_margin: DEFAULT_MIN_MARGIN,
            response: ResponseMode::Reference,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }
}

impl CacheConfig {
    /// Every path off: tools behave exactly as without a cache.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            semantic: false,
            ..Self::default()
        }
    }

    /// From the `MCT_CACHE*` environment variables; an unparsable value is
    /// logged and replaced by its default.
    pub fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// [`CacheConfig::from_env`] over any variable source (tests).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let mut config = Self::default();
        let flag = |name: &str, default: bool| match get(name).as_deref().map(str::trim) {
            None | Some("") => default,
            Some(v) if ["off", "0", "false", "no"].contains(&v.to_ascii_lowercase().as_str()) => {
                false
            }
            Some(v) if ["on", "1", "true", "yes"].contains(&v.to_ascii_lowercase().as_str()) => {
                true
            }
            Some(v) => {
                tracing::warn!(
                    var = name,
                    value = v,
                    "unrecognised cache flag; using default"
                );
                default
            }
        };
        config.enabled = flag(CACHE_ENV, true);
        config.semantic = config.enabled && flag(SEMANTIC_CACHE_ENV, true);
        let number = |name: &str| -> Option<f32> {
            let raw = get(name)?;
            match raw.trim().parse::<f32>() {
                Ok(v) if v.is_finite() => Some(v),
                _ => {
                    tracing::warn!(var = name, value = %raw, "not a number; using default");
                    None
                }
            }
        };
        if let Some(t) = number(THRESHOLD_ENV) {
            config.threshold = t.clamp(MIN_THRESHOLD, 1.0);
        }
        if let Some(m) = number(MARGIN_ENV) {
            config.min_margin = m.clamp(0.0, 1.0);
        }
        match get(RESPONSE_ENV).as_deref().map(str::trim) {
            None | Some("") | Some("reference") => {}
            Some("full") => config.response = ResponseMode::Full,
            Some(other) => {
                tracing::warn!(
                    value = other,
                    "unrecognised {RESPONSE_ENV}; using `reference`"
                )
            }
        }
        if let Some(raw) = get(MAX_ENTRIES_ENV) {
            match raw.trim().parse::<usize>() {
                Ok(n) if n > 0 => config.max_entries = n,
                _ => tracing::warn!(value = %raw, "invalid {MAX_ENTRIES_ENV}; using default"),
            }
        }
        config
    }
}

/// Collapses whitespace runs: `load  config` and ` load config ` rank
/// identically (every search tokenises on whitespace), so they share a key.
/// Case is kept — it steers `alpha` routing (`camelCase` vs prose).
pub fn normalize_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Everything a ranking depends on besides the query text.
#[derive(Debug, Clone)]
pub struct RankingKey<'a> {
    /// Canonical project root — the repository's identity. Its *state* is
    /// validated per lookup instead of hashed in, so an unrelated change can
    /// revalidate an entry rather than orphan it.
    pub repository: &'a str,
    pub tool: &'a str,
    /// Bumped when the tool's ranking or output contract changes.
    pub tool_version: &'a str,
    /// Ranking parameters (scope, `alpha`…), in a fixed order. Presentation
    /// parameters (`limit`, `offset`, `snippet_lines`, `format`) are left
    /// out: a hit is re-rendered with the call's own.
    pub params: Vec<(&'a str, String)>,
    /// Search constants the ranking uses (RRF k, candidate counts…).
    pub search_config: String,
    /// Embedding model (and embedded-text version) of the semantic side;
    /// `None` when the ranking is lexical only.
    pub model: Option<String>,
}

impl RankingKey<'_> {
    fn hash_without_query(&self) -> Hasher {
        let mut h = Hasher::new();
        h.field(CACHE_FORMAT_VERSION)
            .field(env!("CARGO_PKG_VERSION"))
            .field(self.repository)
            .field(self.tool)
            .field(self.tool_version)
            .field(&self.search_config)
            .opt(self.model.as_deref());
        for (name, value) in &self.params {
            h.field(name).field(value);
        }
        h
    }

    /// Key of the queries a semantic candidate may be drawn from.
    pub fn compat_digest(&self) -> Digest {
        self.hash_without_query().finish()
    }

    /// The exact-cache key for `normalized_query`.
    pub fn exact_digest(&self, normalized_query: &str) -> Digest {
        let mut h = self.hash_without_query();
        h.field("query").field(normalized_query);
        h.finish()
    }
}

/// How a cached ranking was found.
#[derive(Debug, Clone, PartialEq)]
pub enum HitKind {
    Exact,
    Semantic {
        similarity: f32,
        /// The earlier query whose ranking is being reused.
        source_query: String,
    },
}

#[derive(Debug, Clone)]
pub struct CachedResult {
    pub hits: Vec<SymbolHit>,
    pub kind: HitKind,
}

/// Why a candidate entry was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    Stale,
    FingerprintMismatch,
    RepositoryMismatch,
}

/// Count, total and worst case of one timed step.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LatencyStats {
    pub count: u64,
    pub total: Duration,
    pub max: Duration,
}

impl LatencyStats {
    fn record(&mut self, elapsed: Duration) {
        self.count += 1;
        self.total += elapsed;
        self.max = self.max.max(elapsed);
    }

    pub fn mean(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            self.total / u32::try_from(self.count).unwrap_or(u32::MAX)
        }
    }
}

impl fmt::Display for LatencyStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} × mean {:.3} ms, max {:.3} ms",
            self.count,
            self.mean().as_secs_f64() * 1e3,
            self.max.as_secs_f64() * 1e3
        )
    }
}

/// The cache's counters. A lookup that ends in a rejection counts both the
/// rejection and the path's miss.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CacheStats {
    pub exact_hit: u64,
    pub exact_miss: u64,
    pub semantic_candidate: u64,
    pub semantic_hit: u64,
    pub semantic_miss: u64,
    pub stale_rejection: u64,
    pub fingerprint_mismatch: u64,
    pub repository_mismatch: u64,
    pub ambiguous_match: u64,
    /// Hits answered with the one-line "unchanged" reply.
    pub reference_responses: u64,
    /// A whole exact or semantic lookup, validation included, embedding not.
    pub lookup_latency: LatencyStats,
    /// Embedding a query for the semantic lookup.
    pub embedding_latency: LatencyStats,
    /// Validating one candidate entry.
    pub validation_latency: LatencyStats,
    pub entries: usize,
}

impl fmt::Display for CacheStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "  exact: {} hit, {} miss; semantic: {} candidate, {} hit, {} miss; {} entries",
            self.exact_hit,
            self.exact_miss,
            self.semantic_candidate,
            self.semantic_hit,
            self.semantic_miss,
            self.entries
        )?;
        writeln!(
            f,
            "  rejected: {} stale, {} fingerprint mismatch, {} repository mismatch, {} ambiguous; {} reference replies",
            self.stale_rejection,
            self.fingerprint_mismatch,
            self.repository_mismatch,
            self.ambiguous_match,
            self.reference_responses
        )?;
        writeln!(f, "  lookup: {}", self.lookup_latency)?;
        writeln!(f, "  embedding: {}", self.embedding_latency)?;
        write!(f, "  validation: {}", self.validation_latency)
    }
}

struct Entry {
    /// The normalised query the ranking was computed for.
    query: String,
    compat: Digest,
    /// L2-normalised query embedding; `None` for a lexical ranking.
    vector: Option<Vec<f32>>,
    /// Repository state the ranking was computed (or last revalidated) at.
    repo_state: Digest,
    hits: Vec<SymbolHit>,
    fingerprint: Digest,
    last_used: u64,
}

/// The repository state as the index sees it, memoised on
/// [`Index::change_marker`] so an idle lookup doesn't re-read every file row.
#[derive(Default)]
struct RepoTracker {
    marker: Option<(i64, i64)>,
    state: Digest,
    files: HashMap<String, String>,
}

impl RepoTracker {
    fn refresh(&mut self, index: &Index) -> Option<()> {
        let marker = index.change_marker().ok()?;
        if self.marker == Some(marker) {
            return Some(());
        }
        let rows = index.file_hashes().ok()?;
        let mut h = Hasher::new();
        for (path, hash) in &rows {
            h.field(path).field(hash);
        }
        self.state = h.finish();
        self.files = rows.into_iter().collect();
        self.marker = Some(marker);
        Some(())
    }
}

/// Recomputes a ranking for `(query, vector)` against the current index;
/// `None` when it can't (then the entry is refused).
pub type Rerank<'r> = dyn FnMut(&str, Option<&[f32]>) -> Option<Vec<SymbolHit>> + 'r;

/// The identity a ranked symbol must still have to count as existing.
fn identity(hit: &SymbolHit) -> (&str, &str, &str, u32, u32) {
    (
        hit.relative_path.as_str(),
        hit.name.as_str(),
        hit.kind.as_str(),
        hit.line,
        hit.column,
    )
}

fn same_ranking(a: &[SymbolHit], b: &[SymbolHit]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            identity(x) == identity(y)
                && x.end_line == y.end_line
                && x.parent == y.parent
                && x.level == y.level
                && x.language == y.language
        })
}

/// Fingerprint of a ranking as the index holds it *now*: each ranked
/// symbol's current row, in rank order, plus the current content hash of
/// every file holding one. `Err(Stale)` when a ranked symbol no longer
/// exists or its file left the index.
fn fingerprint(
    index: &Index,
    files: &HashMap<String, String>,
    hits: &[SymbolHit],
) -> Result<Digest, Rejection> {
    let mut paths: Vec<&str> = hits.iter().map(|h| h.relative_path.as_str()).collect();
    paths.sort_unstable();
    paths.dedup();
    let rows = index
        .symbols_in_files(&paths)
        .map_err(|_| Rejection::RepositoryMismatch)?;
    let current: HashMap<_, &SymbolHit> = rows.iter().map(|r| (identity(r), r)).collect();
    let mut h = Hasher::new();
    for hit in hits {
        let row = current.get(&identity(hit)).ok_or(Rejection::Stale)?;
        let content_hash = files.get(&hit.relative_path).ok_or(Rejection::Stale)?;
        h.field(&row.relative_path)
            .field(content_hash)
            .field(&row.name)
            .field(&row.kind)
            .field(&row.language)
            .field(row.line.to_le_bytes())
            .field(row.column.to_le_bytes())
            .field(row.end_line.unwrap_or(0).to_le_bytes())
            .field(row.level.unwrap_or(0).to_le_bytes())
            .opt(row.parent.as_deref());
    }
    Ok(h.finish())
}

fn normalized(vector: &[f32]) -> Vec<f32> {
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 && norm.is_finite() {
        vector.iter().map(|x| x / norm).collect()
    } else {
        vector.to_vec()
    }
}

fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    (a.len() == b.len() && !a.is_empty()).then(|| a.iter().zip(b).map(|(x, y)| x * y).sum())
}

/// The cache itself. Not thread-safe on its own; the server keeps it behind
/// a mutex taken while it holds the index lock.
pub struct QueryCache {
    config: CacheConfig,
    entries: HashMap<Digest, Entry>,
    repo: RepoTracker,
    sent: HashSet<Digest>,
    sent_order: std::collections::VecDeque<Digest>,
    clock: u64,
    stats: CacheStats,
}

impl QueryCache {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            entries: HashMap::new(),
            repo: RepoTracker::default(),
            sent: HashSet::new(),
            sent_order: std::collections::VecDeque::new(),
            clock: 0,
            stats: CacheStats::default(),
        }
    }

    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.entries.len(),
            ..self.stats.clone()
        }
    }

    pub fn record_embedding(&mut self, elapsed: Duration) {
        self.stats.embedding_latency.record(elapsed);
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    /// Exact lookup of `normalized_query` under `key`, validated.
    pub fn lookup_exact(
        &mut self,
        index: &Index,
        key: &RankingKey<'_>,
        normalized_query: &str,
        rerank: &mut Rerank<'_>,
    ) -> Option<CachedResult> {
        if !self.config.enabled {
            return None;
        }
        let start = Instant::now();
        let digest = key.exact_digest(normalized_query);
        let result = if self.entries.contains_key(&digest) {
            self.validate(index, &digest, rerank)
                .map(|hits| CachedResult {
                    hits,
                    kind: HitKind::Exact,
                })
        } else {
            None
        };
        if result.is_some() {
            self.stats.exact_hit += 1;
        } else {
            self.stats.exact_miss += 1;
        }
        self.stats.lookup_latency.record(start.elapsed());
        result
    }

    /// Semantic lookup: the nearest earlier query under the same key minus
    /// the query text, if it clears the threshold, is unambiguous and
    /// validates.
    pub fn lookup_semantic(
        &mut self,
        index: &Index,
        key: &RankingKey<'_>,
        normalized_query: &str,
        query_vector: &[f32],
        rerank: &mut Rerank<'_>,
    ) -> Option<CachedResult> {
        if !self.config.enabled || !self.config.semantic || key.model.is_none() {
            return None;
        }
        let start = Instant::now();
        let compat = key.compat_digest();
        let query_vector = normalized(query_vector);
        let mut scored: Vec<(f32, Digest)> = self
            .entries
            .iter()
            .filter(|(_, e)| e.compat == compat && e.query != normalized_query)
            .filter_map(|(d, e)| Some((cosine(&query_vector, e.vector.as_deref()?)?, *d)))
            .filter(|(s, _)| s.is_finite())
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let result = match scored.first() {
            Some(&(best, digest)) if best >= self.config.threshold => {
                self.stats.semantic_candidate += 1;
                let runner_up = scored.get(1).map(|s| s.0);
                if runner_up.is_some_and(|second| best - second < self.config.min_margin) {
                    self.stats.ambiguous_match += 1;
                    None
                } else {
                    let source_query = self
                        .entries
                        .get(&digest)
                        .map(|e| e.query.clone())
                        .unwrap_or_default();
                    self.validate(index, &digest, rerank)
                        .map(|hits| CachedResult {
                            hits,
                            kind: HitKind::Semantic {
                                similarity: best,
                                source_query,
                            },
                        })
                }
            }
            _ => None,
        };
        if result.is_some() {
            self.stats.semantic_hit += 1;
        } else {
            self.stats.semantic_miss += 1;
        }
        self.stats.lookup_latency.record(start.elapsed());
        result
    }

    /// Runs the validation chain on one entry; a refused entry is dropped.
    fn validate(
        &mut self,
        index: &Index,
        digest: &Digest,
        rerank: &mut Rerank<'_>,
    ) -> Option<Vec<SymbolHit>> {
        let start = Instant::now();
        let outcome = self.check(index, digest, rerank);
        self.stats.validation_latency.record(start.elapsed());
        match outcome {
            Ok(hits) => {
                let now = self.tick();
                if let Some(entry) = self.entries.get_mut(digest) {
                    entry.last_used = now;
                }
                Some(hits)
            }
            Err(rejection) => {
                match rejection {
                    Rejection::Stale => self.stats.stale_rejection += 1,
                    Rejection::FingerprintMismatch => self.stats.fingerprint_mismatch += 1,
                    Rejection::RepositoryMismatch => self.stats.repository_mismatch += 1,
                }
                self.entries.remove(digest);
                None
            }
        }
    }

    fn check(
        &mut self,
        index: &Index,
        digest: &Digest,
        rerank: &mut Rerank<'_>,
    ) -> Result<Vec<SymbolHit>, Rejection> {
        self.repo
            .refresh(index)
            .ok_or(Rejection::RepositoryMismatch)?;
        let entry = self
            .entries
            .get(digest)
            .ok_or(Rejection::RepositoryMismatch)?;
        // 2 + 3: the ranked symbols still exist, unchanged, in unchanged files.
        if fingerprint(index, &self.repo.files, &entry.hits)? != entry.fingerprint {
            return Err(Rejection::FingerprintMismatch);
        }
        if entry.repo_state == self.repo.state {
            return Ok(entry.hits.clone());
        }
        // 4: something else changed — does the ranking still come out the same?
        let fresh =
            rerank(&entry.query, entry.vector.as_deref()).ok_or(Rejection::RepositoryMismatch)?;
        if !same_ranking(&fresh, &entry.hits) {
            return Err(Rejection::RepositoryMismatch);
        }
        let state = self.repo.state;
        if let Some(entry) = self.entries.get_mut(digest) {
            entry.repo_state = state;
        }
        Ok(fresh)
    }

    /// Caches `hits` as the ranking of `normalized_query` under `key`,
    /// stamped with the current repository state and fingerprint.
    pub fn insert(
        &mut self,
        index: &Index,
        key: &RankingKey<'_>,
        normalized_query: &str,
        query_vector: Option<&[f32]>,
        hits: Vec<SymbolHit>,
    ) {
        if !self.config.enabled {
            return;
        }
        if self.repo.refresh(index).is_none() {
            return;
        }
        let Ok(fingerprint) = fingerprint(index, &self.repo.files, &hits) else {
            return;
        };
        if self.entries.len() >= self.config.max_entries {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(d, _)| *d)
            {
                self.entries.remove(&oldest);
            }
        }
        let now = self.tick();
        self.entries.insert(
            key.exact_digest(normalized_query),
            Entry {
                query: normalized_query.to_string(),
                compat: key.compat_digest(),
                vector: query_vector.map(normalized),
                repo_state: self.repo.state,
                hits,
                fingerprint,
                last_used: now,
            },
        );
    }

    /// Whether this session was already sent a response whose
    /// query-independent rendering is `rendered`.
    pub fn was_sent(&self, rendered: &str) -> bool {
        self.sent.contains(&sha256(rendered.as_bytes()))
    }

    pub fn record_sent(&mut self, rendered: &str) {
        let digest = sha256(rendered.as_bytes());
        if self.sent.insert(digest) {
            self.sent_order.push_back(digest);
            if self.sent_order.len() > MAX_SENT {
                if let Some(old) = self.sent_order.pop_front() {
                    self.sent.remove(&old);
                }
            }
        }
    }

    pub fn record_reference(&mut self) {
        self.stats.reference_responses += 1;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn key<'a>() -> RankingKey<'a> {
        RankingKey {
            repository: "/repo",
            tool: "hybrid_search",
            tool_version: "1",
            params: vec![("path", String::new()), ("alpha", "0.75".to_string())],
            search_config: "rrf=10".to_string(),
            model: Some("m1".to_string()),
        }
    }

    #[test]
    fn every_key_part_changes_the_exact_digest() {
        let base = key().exact_digest("load config");
        let variants = [
            RankingKey {
                repository: "/other",
                ..key()
            },
            RankingKey {
                tool: "search_symbols",
                ..key()
            },
            RankingKey {
                tool_version: "2",
                ..key()
            },
            RankingKey {
                params: vec![("path", "src".to_string()), ("alpha", "0.75".to_string())],
                ..key()
            },
            RankingKey {
                params: vec![("path", String::new()), ("alpha", "0.5".to_string())],
                ..key()
            },
            RankingKey {
                search_config: "rrf=60".to_string(),
                ..key()
            },
            RankingKey {
                model: Some("m2".to_string()),
                ..key()
            },
            RankingKey {
                model: None,
                ..key()
            },
        ];
        for variant in &variants {
            assert_ne!(variant.exact_digest("load config"), base, "{variant:?}");
            assert_ne!(
                variant.compat_digest(),
                key().compat_digest(),
                "{variant:?}"
            );
        }
        assert_ne!(key().exact_digest("load configs"), base);
        assert_eq!(key().exact_digest("load config"), base);
    }

    #[test]
    fn length_prefixing_keeps_adjacent_fields_apart() {
        let a = RankingKey {
            params: vec![("ab", "c".to_string())],
            ..key()
        };
        let b = RankingKey {
            params: vec![("a", "bc".to_string())],
            ..key()
        };
        assert_ne!(a.exact_digest("q"), b.exact_digest("q"));
    }

    #[test]
    fn whitespace_is_normalised_but_case_is_kept() {
        assert_eq!(normalize_query("  load \t config\n"), "load config");
        assert_ne!(normalize_query("parseReq"), normalize_query("parsereq"));
    }

    #[test]
    fn config_reads_the_environment_and_rejects_nonsense() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(CacheConfig::from_lookup(env(&[])), CacheConfig::default());

        let off = CacheConfig::from_lookup(env(&[(CACHE_ENV, "off")]));
        assert!(!off.enabled && !off.semantic);

        let exact_only = CacheConfig::from_lookup(env(&[(SEMANTIC_CACHE_ENV, "0")]));
        assert!(exact_only.enabled && !exact_only.semantic);

        let tuned = CacheConfig::from_lookup(env(&[
            (THRESHOLD_ENV, "0.97"),
            (MARGIN_ENV, "0.05"),
            (RESPONSE_ENV, "full"),
            (MAX_ENTRIES_ENV, "8"),
        ]));
        assert_eq!(tuned.threshold, 0.97);
        assert_eq!(tuned.min_margin, 0.05);
        assert_eq!(tuned.response, ResponseMode::Full);
        assert_eq!(tuned.max_entries, 8);

        let clamped = CacheConfig::from_lookup(env(&[(THRESHOLD_ENV, "0.1")]));
        assert_eq!(clamped.threshold, MIN_THRESHOLD);
        let garbage = CacheConfig::from_lookup(env(&[
            (THRESHOLD_ENV, "high"),
            (CACHE_ENV, "maybe"),
            (MAX_ENTRIES_ENV, "0"),
        ]));
        assert_eq!(garbage, CacheConfig::default());
    }

    #[test]
    fn cosine_refuses_mismatched_dimensions() {
        assert_eq!(cosine(&[1.0, 0.0], &[1.0, 0.0, 0.0]), None);
        let a = normalized(&[3.0, 4.0]);
        assert!((cosine(&a, &a).unwrap() - 1.0).abs() < 1e-6);
        assert_eq!(normalized(&[0.0, 0.0]), vec![0.0, 0.0]);
    }
}
