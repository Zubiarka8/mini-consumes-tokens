//! SQLite-backed storage and query layer for the symbol graph. Same schema
//! for every language (a `language` column on `files`, not per-language
//! tables) — this crate knows about SQLite and git, never about any specific
//! language's grammar.

mod dead_code;
mod error;
mod exclude;
mod file_tree;
mod indexer;
mod manifests;
mod queries;
mod schema;
mod search;
mod semantic;
mod traversal;

pub use dead_code::{
    find_dead_code_candidates, looks_like_test_name, DEAD_CODE_ENTRY_POINT_NAMES,
    DEAD_CODE_KIND_ALLOWLIST,
};
pub use error::{IndexError, Result};
pub use exclude::{
    read_ignore_file, ExcludeSet, GITIGNORE_IMPORT_DIRECTIVE, IGNORE_FILE_NAME,
    IGNORE_FILE_TEMPLATE,
};
pub use file_tree::FileTreeNode;
pub use indexer::{
    DependencyInfo, IndexStatus, LanguageCoverage, ManifestDependencies, ReindexReport,
    UnsupportedFile, UnsupportedKind,
};
pub use queries::{
    CandidateRef, QueryScope, RelationHit, Resolution, SymbolHit, SymbolListEntry, SymbolMatchMode,
    SHOWN_CANDIDATES,
};
pub use search::{exact_phrase, search_words, split_identifier, LiteralHit, MAX_LITERAL_HITS};
pub use semantic::{
    classify_query, embedding_text, Embedder, EmbeddingCoverage, HybridHit, PendingEmbeddings,
    QueryIntent, SymbolContext, EMBEDDING_TEXT_VERSION, EXACT_PHRASE_BOOST, RRF_K,
    SEMANTIC_CANDIDATES,
};

use queries::ResolvedScope;

use std::path::{Path, PathBuf};

use mct_core::LanguageRegistry;
use rusqlite::Connection;

/// Early-stop budget for a `find_*_bfs` walk. At `depth <= 1` the walk is a
/// single query identical to the plain single-hop method, which has always
/// returned every direct hit uncapped — that invariant must hold regardless
/// of `limit`/`offset`, or the reported total would silently shrink to the
/// budget. Only a genuine multi-hop walk (`depth > 1`) is bounded, since an
/// unbounded traversal has no pre-existing "true total" to preserve.
fn bfs_budget(depth: u32, limit: usize, offset: usize) -> usize {
    if depth <= 1 {
        usize::MAX
    } else {
        limit.saturating_add(offset)
    }
}

/// Prepared-statement cache capacity, raised from rusqlite's default of 16.
///
/// The query layer uses `prepare_cached`, which keys on the SQL string, and
/// scoping multiplies the distinct shapes: each of the four lookups can be
/// built with no path predicate, an exact-file one or a directory-prefix one,
/// times with-or-without a language predicate, plus `list_symbols`' own
/// kind/language combinations. That is a few dozen shapes in total — a fixed,
/// statically bounded set, since only static fragments are ever concatenated —
/// so one cache slot each keeps a multi-hop BFS from recompiling identical SQL
/// at every level (§C3 of `investigacion.md`).
const STATEMENT_CACHE_CAPACITY: usize = 64;

/// One open connection to a project's `.mct-index/index.sqlite3`.
pub struct Index {
    conn: Connection,
    /// Canonicalized project root — every relative path stored in the
    /// database is validated against this to reject path traversal.
    root: PathBuf,
    exclude: ExcludeSet,
}

impl Index {
    /// Opens (creating if needed) the index database at `db_path` for the
    /// project rooted at `root`, applying pending migrations.
    pub fn open(root: &Path, db_path: &Path, exclude: ExcludeSet) -> Result<Self> {
        let root = root.canonicalize().map_err(|source| IndexError::Io {
            path: root.display().to_string(),
            source,
        })?;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| IndexError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
        let mut conn = Connection::open(db_path)?;
        conn.set_prepared_statement_cache_capacity(STATEMENT_CACHE_CAPACITY);
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        search::register_functions(&conn)?;
        schema::migrations().to_latest(&mut conn)?;
        Ok(Self {
            conn,
            root,
            exclude,
        })
    }

    /// Opens an in-memory index, used by tests.
    #[doc(hidden)]
    pub fn open_in_memory(root: &Path, exclude: ExcludeSet) -> Result<Self> {
        let root = root.canonicalize().map_err(|source| IndexError::Io {
            path: root.display().to_string(),
            source,
        })?;
        let mut conn = Connection::open_in_memory()?;
        conn.set_prepared_statement_cache_capacity(STATEMENT_CACHE_CAPACITY);
        conn.pragma_update(None, "foreign_keys", "ON")?;
        search::register_functions(&conn)?;
        schema::migrations().to_latest(&mut conn)?;
        Ok(Self {
            conn,
            root,
            exclude,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether `path` (relative to [`Index::root`]) is a directory on disk.
    ///
    /// The query layer needs this to tell "exact file" from "directory
    /// prefix": the old last-segment-contains-a-dot heuristic classified
    /// `.claude`, `.github`, `.cargo` or `v1.2/` as files and silently
    /// returned nothing for them (§B5 of `investigacion.md`). Public because
    /// the MCP server makes the same formatting decision.
    pub fn path_is_directory(&self, path: &str) -> bool {
        self.root.join(path).is_dir()
    }

    /// Resolves `path` to "match this exact file" (`true`) vs "match this
    /// directory prefix" (`false`).
    ///
    /// The filesystem is authoritative; the historical string heuristic is
    /// used only when the path no longer exists on disk, so a deleted —
    /// but still indexed — path keeps resolving the way it always did.
    fn resolve_is_file(&self, path: &str) -> bool {
        let joined = self.root.join(path);
        if joined.is_dir() {
            return false;
        }
        if joined.is_file() {
            return true;
        }
        path.rsplit('/').next().unwrap_or(path).contains('.')
    }

    /// Resolves a caller-supplied [`QueryScope`] against the filesystem once,
    /// so a multi-hop walk doesn't re-`stat` the same path at every level.
    fn resolve_scope<'a>(&self, scope: QueryScope<'a>) -> ResolvedScope<'a> {
        ResolvedScope {
            path: scope.path,
            path_is_file: scope.path.is_some_and(|p| self.resolve_is_file(p)),
            language: scope.language,
        }
    }

    /// Walks the project, parsing every file whose content changed since the
    /// last run (or every supported file, if `force`), via the parser the
    /// `registry` resolves for each extension. Never touches a file outside
    /// [`Index::root`], and never follows a symlink that would escape it.
    /// First re-reads the project's ignore files ([`ExcludeSet::reload`]), so
    /// a rule added or removed since the index was opened applies to this run.
    pub fn reindex(&mut self, registry: &LanguageRegistry, force: bool) -> Result<ReindexReport> {
        indexer::reindex(self, registry, force)
    }

    /// Incremental [`Index::reindex`]: updates the index for `paths` only
    /// (absolute, or relative to [`Index::root`]) — changed, created,
    /// deleted or renamed files and directories — without walking the rest
    /// of the project. For those paths, the same result as a full reindex;
    /// see [`indexer::reindex_paths`].
    pub fn reindex_paths(
        &mut self,
        registry: &LanguageRegistry,
        paths: &[PathBuf],
    ) -> Result<ReindexReport> {
        indexer::reindex_paths(self, registry, paths)
    }

    pub fn status(&self) -> Result<IndexStatus> {
        indexer::status(self)
    }

    pub fn find_symbol(&self, name: &str) -> Result<Vec<SymbolHit>> {
        self.find_symbol_scoped(name, QueryScope::default())
    }

    /// [`Index::find_symbol`], widened by `mode` — see [`SymbolMatchMode`].
    pub fn find_symbol_matching(
        &self,
        name: &str,
        mode: SymbolMatchMode,
    ) -> Result<Vec<SymbolHit>> {
        queries::find_symbol_matching(&self.conn, name, mode)
    }

    /// [`Index::find_symbol_matching`] narrowed to `scope` — see
    /// [`Index::find_symbol_scoped`]. With an empty scope, identical hit for
    /// hit.
    pub fn find_symbol_matching_scoped(
        &self,
        name: &str,
        mode: SymbolMatchMode,
        scope: QueryScope<'_>,
    ) -> Result<Vec<SymbolHit>> {
        queries::find_symbol_matching_scoped(&self.conn, name, mode, self.resolve_scope(scope))
    }

    /// Ranked lexical search over split symbol names, narrowed to `scope` —
    /// finds `parseRequestBody` for `parse request`, `parse_req` or
    /// `ParseRequest`, which every [`SymbolMatchMode`] misses. Exact
    /// (case-insensitive) name matches always rank first. See
    /// [`search::search_symbols`] for the full ranking.
    pub fn search_symbols(&self, query: &str, scope: QueryScope<'_>) -> Result<Vec<SymbolHit>> {
        search::search_symbols(&self.conn, query, self.resolve_scope(scope))
    }

    /// Embeds every symbol with no vector yet for `embedder`'s model and
    /// drops other models' vectors; returns how many were embedded. Cheap
    /// after the first call — only symbols the last reindex added or
    /// rewrote are pending.
    pub fn refresh_embeddings(&self, embedder: &dyn Embedder) -> Result<usize> {
        semantic::refresh_embeddings(&self.conn, &self.root, embedder)
    }

    /// Step 1 of a refresh that must not hold the connection while the model
    /// runs: the symbols with no vector for `model_id`. See
    /// [`PendingEmbeddings::embed_batch`] and [`Index::commit_embedding_batch`].
    pub fn snapshot_pending_embeddings(&self, model_id: &str) -> Result<PendingEmbeddings> {
        semantic::snapshot_pending_embeddings(&self.conn, &self.root, model_id)
    }

    /// Step 3 of a refresh: stores one batch's vectors, skipping any symbol
    /// that changed since the snapshot. Returns how many were stored.
    pub fn commit_embedding_batch(
        &self,
        pending: &PendingEmbeddings,
        batch: usize,
        vectors: Vec<Vec<f32>>,
    ) -> Result<usize> {
        semantic::commit_embedding_batch(&self.conn, pending, batch, vectors)
    }

    /// How many symbols have a vector for `model`, out of how many exist.
    pub fn embedding_coverage(&self, model: &str) -> Result<EmbeddingCoverage> {
        semantic::embedding_coverage(&self.conn, model)
    }

    /// [`Index::search_symbols`] fused with an embedding-similarity ranking
    /// by weighted Reciprocal Rank Fusion, narrowed to `scope`. `alpha`
    /// weighs the two (0 = lexical only, 1 = semantic only). With no
    /// `embedder`, or `alpha == 0`, the lexical ranking is returned
    /// unchanged — see [`semantic::hybrid_search`].
    pub fn hybrid_search(
        &self,
        query: &str,
        embedder: Option<&dyn Embedder>,
        alpha: f64,
        scope: QueryScope<'_>,
    ) -> Result<Vec<HybridHit>> {
        let query_vector = match embedder {
            Some(embedder) if alpha > 0.0 => {
                Some((Self::embed_query(embedder, query)?, embedder.model_id()))
            }
            _ => None,
        };
        self.hybrid_search_with_vector(
            query,
            query_vector
                .as_ref()
                .map(|(v, model)| (v.as_slice(), *model)),
            alpha,
            scope,
        )
    }

    /// The vector [`Index::hybrid_search`] ranks `query` by: its identifier
    /// words, space-joined, embedded as a retrieval query.
    pub fn embed_query(embedder: &dyn Embedder, query: &str) -> Result<Vec<f32>> {
        let text = split_identifier(query).join(" ");
        embedder
            .embed_query(if text.is_empty() { query } else { &text })
            .map_err(IndexError::Embedding)
    }

    /// [`Index::hybrid_search`] with the query already embedded (by
    /// [`Index::embed_query`], for `model`), so a caller that needs the
    /// vector for something else too — `mct-mcp-server`'s query cache —
    /// embeds it once. `None` is the lexical ranking.
    pub fn hybrid_search_with_vector(
        &self,
        query: &str,
        query_vector: Option<(&[f32], &str)>,
        alpha: f64,
        scope: QueryScope<'_>,
    ) -> Result<Vec<HybridHit>> {
        semantic::hybrid_search(
            &self.conn,
            query,
            query_vector,
            alpha,
            self.resolve_scope(scope),
        )
    }

    /// Every symbol defined in any of `paths` — exact relative paths, never
    /// prefixes — ordered by path then line. One query however many paths;
    /// `mct-mcp-server`'s query cache uses it to check a cached result's
    /// symbols still exist as they were.
    pub fn symbols_in_files(&self, paths: &[&str]) -> Result<Vec<SymbolHit>> {
        queries::symbols_in_files(&self.conn, paths)
    }

    /// Every indexed file's `(relative_path, content_hash)`, sorted by path —
    /// the index's view of the repository state. `mct-mcp-server`'s query
    /// cache fingerprints it to tell whether a cached result was computed
    /// against the index as it is now, and diffs two of them to find which
    /// files a reindex changed.
    pub fn file_hashes(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT relative_path, content_hash FROM files ORDER BY relative_path",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// A cheap marker that changes whenever the database may have changed:
    /// this connection's own row writes (`total_changes()`) plus
    /// `PRAGMA data_version`, which moves when another connection (e.g. a
    /// `mct-cli reindex` in another process) commits. Equal markers mean
    /// [`Index::file_hashes`] can't have changed, so a caller can skip
    /// re-reading it; different markers only mean it *might* have.
    pub fn change_marker(&self) -> Result<(i64, i64)> {
        let own: i64 = self
            .conn
            .query_row("SELECT total_changes()", [], |row| row.get(0))?;
        let others: i64 = self
            .conn
            .pragma_query_value(None, "data_version", |row| row.get(0))?;
        Ok((own, others))
    }

    /// Every indexed prose string literal holding `phrase` as consecutive
    /// words, narrowed to `scope` — `hybrid_search`'s exact-phrase mode
    /// reports these alongside matching symbol names. See
    /// [`search::search_literals`].
    pub fn search_literals(&self, phrase: &str, scope: QueryScope<'_>) -> Result<Vec<LiteralHit>> {
        search::search_literals(&self.conn, phrase, self.resolve_scope(scope))
    }

    /// Every place `symbol` is referenced: calls, imports, extends/implements,
    /// and plain references — a superset of [`Index::find_calls`].
    pub fn find_references(&self, symbol: &str) -> Result<Vec<RelationHit>> {
        self.find_references_scoped(symbol, QueryScope::default())
    }

    /// Calls made *by* `function` (its callees).
    pub fn find_calls(&self, function: &str) -> Result<Vec<RelationHit>> {
        self.find_calls_scoped(function, QueryScope::default())
    }

    /// Calls made *of* `function` (its callers) — the inverse of [`Index::find_calls`].
    pub fn find_callers(&self, function: &str) -> Result<Vec<RelationHit>> {
        self.find_callers_scoped(function, QueryScope::default())
    }

    /// [`Index::find_symbol`] narrowed to `scope`. `QueryScope::default()` is
    /// the unscoped query, hit for hit.
    pub fn find_symbol_scoped(&self, name: &str, scope: QueryScope<'_>) -> Result<Vec<SymbolHit>> {
        queries::find_symbol_scoped(&self.conn, name, self.resolve_scope(scope))
    }

    /// [`Index::find_references`] narrowed to `scope`.
    ///
    /// The scope matches the file holding the *referring* symbol — the file
    /// the caller is scoping to — not the file the reference resolves to.
    /// Hits are matched by spelling (`to_name`), so without a scope a query
    /// for a common name (`main`, `new`, `run`) also lists relations to other
    /// same-named symbols; each hit's [`RelationHit::resolution`] says which
    /// definition, if any, it was proven to denote (§B1 of `investigacion.md`).
    pub fn find_references_scoped(
        &self,
        symbol: &str,
        scope: QueryScope<'_>,
    ) -> Result<Vec<RelationHit>> {
        queries::find_references_scoped(&self.conn, symbol, self.resolve_scope(scope))
    }

    /// [`Index::find_calls`] narrowed to `scope`. See
    /// [`Index::find_references_scoped`] for which file the scope matches.
    pub fn find_calls_scoped(
        &self,
        function: &str,
        scope: QueryScope<'_>,
    ) -> Result<Vec<RelationHit>> {
        queries::find_calls_scoped(&self.conn, function, self.resolve_scope(scope))
    }

    /// [`Index::find_callers`] narrowed to `scope`. See
    /// [`Index::find_references_scoped`] for which file the scope matches.
    pub fn find_callers_scoped(
        &self,
        function: &str,
        scope: QueryScope<'_>,
    ) -> Result<Vec<RelationHit>> {
        queries::find_callers_scoped(&self.conn, function, self.resolve_scope(scope))
    }

    /// Non-call relations made *by* `symbol`: what it imports, extends,
    /// implements or otherwise references. See
    /// [`Index::find_references_scoped`] for which file the scope matches.
    pub fn find_dependencies_scoped(
        &self,
        symbol: &str,
        scope: QueryScope<'_>,
    ) -> Result<Vec<RelationHit>> {
        queries::find_dependencies_scoped(&self.conn, symbol, self.resolve_scope(scope))
    }

    /// The candidate definitions of one relation ([`RelationHit::relation_id`]):
    /// its single target when resolved, every remaining candidate when
    /// ambiguous, none when unresolved or external.
    pub fn relation_candidates(&self, relation_id: i64) -> Result<Vec<SymbolHit>> {
        queries::relation_candidates(&self.conn, relation_id)
    }

    /// The symbols with the given row ids ([`SymbolHit::id`],
    /// [`RelationHit::target_id`], [`RelationHit::from_symbol_id`]), in id
    /// order. Ids from before a reindex of their file are simply absent.
    pub fn symbols_by_ids(&self, ids: &[i64]) -> Result<Vec<SymbolHit>> {
        queries::symbols_by_ids(&self.conn, ids)
    }

    /// Calls made by the exact symbol rows `start` (not by every definition
    /// sharing their name), walked forward along resolved callees only, up
    /// to `depth` hops and `limit` hits. See `traversal.rs`.
    pub fn find_calls_from_symbols(
        &self,
        start: &[i64],
        depth: u32,
        limit: usize,
    ) -> Result<Vec<RelationHit>> {
        traversal::find_calls_from(self, start, depth, limit)
    }

    /// Calls that may reach the exact symbol rows `start` — resolved to one
    /// of them, or ambiguous with one among the candidates — walked backward
    /// from proven callers only, up to `depth` hops and `limit` hits.
    pub fn find_callers_of_symbols(
        &self,
        start: &[i64],
        depth: u32,
        limit: usize,
    ) -> Result<Vec<RelationHit>> {
        traversal::find_referrers_of(self, start, true, depth, limit)
    }

    /// [`Index::find_callers_of_symbols`] over every relation kind.
    pub fn find_references_of_symbols(
        &self,
        start: &[i64],
        depth: u32,
        limit: usize,
    ) -> Result<Vec<RelationHit>> {
        traversal::find_referrers_of(self, start, false, depth, limit)
    }

    /// Non-call relations made by the exact symbol rows `start`.
    pub fn find_dependencies_of_symbols(&self, start: &[i64]) -> Result<Vec<RelationHit>> {
        let mut hits = Vec::new();
        for &id in start {
            hits.extend(queries::dependencies_of_symbol(&self.conn, id)?);
        }
        Ok(hits)
    }

    /// Direct-caller counts for every called name, in one aggregate query.
    ///
    /// Replaces the one-`find_callers`-per-candidate-symbol fan-in ranking in
    /// `get_project_overview`, which cost >1.800 queries on this repo (§C4 of
    /// `investigacion.md`). A name absent from the map has zero direct callers.
    pub fn fan_in_counts(&self) -> Result<std::collections::HashMap<String, usize>> {
        queries::fan_in_counts(&self.conn)
    }

    /// Total reference counts for every referenced name, across every
    /// relation kind — the whole-project data `find_dead_code` filters
    /// candidate symbols against. A name absent from the map has zero
    /// references of any kind anywhere in the index.
    pub fn reference_counts(&self) -> Result<std::collections::HashMap<String, usize>> {
        queries::reference_counts(&self.conn)
    }

    /// Multi-hop [`Index::find_calls`]: walks the call graph forward up to
    /// `depth` hops (1 = identical to [`Index::find_calls`] — the full,
    /// uncapped direct-hit list, so a caller that always passes `depth: 1`
    /// sees the exact same total it always has). Beyond depth 1, stops once
    /// `limit + offset` hits are collected, since an unbounded multi-hop
    /// walk has no equivalent "true total" to preserve. `depth` beyond
    /// [`mct_core::MAX_QUERY_DEPTH`] is clamped. Hops past the first follow
    /// only resolved callees (by symbol id, never by name); ambiguous and
    /// unresolved calls are reported but not expanded — see `traversal.rs`.
    pub fn find_calls_bfs(
        &self,
        function: &str,
        depth: u32,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<RelationHit>> {
        self.find_calls_bfs_scoped(function, depth, limit, offset, QueryScope::default())
    }

    /// [`Index::find_calls_bfs`] narrowed to `scope` at the start definitions
    /// and hop 1 only; later hops follow exact symbol ids across files, so
    /// the walk may leave the scope.
    pub fn find_calls_bfs_scoped(
        &self,
        function: &str,
        depth: u32,
        limit: usize,
        offset: usize,
        scope: QueryScope<'_>,
    ) -> Result<Vec<RelationHit>> {
        traversal::find_calls_bfs(
            self,
            function,
            depth,
            bfs_budget(depth, limit, offset),
            self.resolve_scope(scope),
        )
    }

    /// Multi-hop [`Index::find_callers`]: walks the call graph backward up
    /// to `depth` hops (1 = identical to [`Index::find_callers`]). See
    /// [`Index::find_calls_bfs`] for the shared semantics of `depth`/`limit`/`offset`.
    pub fn find_callers_bfs(
        &self,
        function: &str,
        depth: u32,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<RelationHit>> {
        self.find_callers_bfs_scoped(function, depth, limit, offset, QueryScope::default())
    }

    /// [`Index::find_callers_bfs`] narrowed to `scope` at the start and hop 1 only —
    /// see [`Index::find_calls_bfs_scoped`].
    pub fn find_callers_bfs_scoped(
        &self,
        function: &str,
        depth: u32,
        limit: usize,
        offset: usize,
        scope: QueryScope<'_>,
    ) -> Result<Vec<RelationHit>> {
        traversal::find_callers_bfs(
            self,
            function,
            depth,
            bfs_budget(depth, limit, offset),
            self.resolve_scope(scope),
        )
    }

    /// Multi-hop [`Index::find_references`]: walks the reference graph
    /// backward (any relation kind) up to `depth` hops (1 = identical to
    /// [`Index::find_references`]). See [`Index::find_calls_bfs`] for the
    /// shared semantics of `depth`/`limit`/`offset`.
    pub fn find_references_bfs(
        &self,
        symbol: &str,
        depth: u32,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<RelationHit>> {
        self.find_references_bfs_scoped(symbol, depth, limit, offset, QueryScope::default())
    }

    /// [`Index::find_references_bfs`] narrowed to `scope` at the start and hop 1
    /// only — see [`Index::find_calls_bfs_scoped`].
    pub fn find_references_bfs_scoped(
        &self,
        symbol: &str,
        depth: u32,
        limit: usize,
        offset: usize,
        scope: QueryScope<'_>,
    ) -> Result<Vec<RelationHit>> {
        traversal::find_references_bfs(
            self,
            symbol,
            depth,
            bfs_budget(depth, limit, offset),
            self.resolve_scope(scope),
        )
    }

    /// Lists symbol definitions under `path` (a single file or a
    /// directory/crate prefix), optionally filtered by `kind` and/or
    /// `language`. See [`queries::list_symbols`] for the exact matching
    /// rules; whether `path` is a file or a directory is decided by
    /// [`Index::resolve_is_file`], not by the shape of the string.
    pub fn list_symbols(
        &self,
        path: &str,
        kind: Option<&str>,
        language: Option<&str>,
    ) -> Result<Vec<SymbolListEntry>> {
        queries::list_symbols(&self.conn, path, self.resolve_is_file(path), kind, language)
    }

    /// [`Index::list_symbols`], but `path` is optional: `None` lists every
    /// symbol in the whole project.
    ///
    /// `Index::list_symbols`'s directory-prefix matching has no way to
    /// express "everything" on its own — its `LIKE` pattern is always
    /// anchored to a specific prefix, and an empty prefix produces `/%%`
    /// which matches nothing, since stored `relative_path`s never start with
    /// a leading slash. So instead, when `path` is omitted, this walks the
    /// project root's own top-level directory entries (skipping
    /// dotfiles/dot-directories, e.g. `.git`/`.mct-index`) and unions one
    /// `list_symbols` call per entry — each entry is itself a valid
    /// file-or-prefix path, so no new query semantics are needed.
    pub fn list_symbols_all(
        &self,
        path: Option<&str>,
        language: Option<&str>,
    ) -> Result<Vec<SymbolListEntry>> {
        if let Some(path) = path {
            return self.list_symbols(path, None, language);
        }
        let mut top_level: Vec<String> = std::fs::read_dir(&self.root)
            .map_err(|source| IndexError::Io {
                path: self.root.display().to_string(),
                source,
            })?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                (!name.starts_with('.')).then_some(name)
            })
            .collect();
        top_level.sort();

        let mut entries = Vec::new();
        for name in &top_level {
            entries.extend(self.list_symbols(name, None, language)?);
        }
        Ok(entries)
    }

    /// Directory tree rooted at `path` (defaults to the project root), down
    /// to `depth` levels of nesting, pruned by the same [`ExcludeSet`]
    /// `reindex` uses. Pure filesystem listing — never touches the symbol
    /// index — so it stays cheap and correct even for a file the parser
    /// doesn't support. `depth` is clamped to `[1, mct_core::MAX_QUERY_DEPTH]`.
    pub fn file_tree(&self, path: Option<&str>, depth: u32) -> Result<FileTreeNode> {
        let depth = depth.clamp(1, mct_core::MAX_QUERY_DEPTH);
        let relative_start = path.map(str::trim).filter(|p| !p.is_empty()).unwrap_or(".");
        file_tree::file_tree(&self.root, &self.exclude, relative_start, depth)
    }
}
