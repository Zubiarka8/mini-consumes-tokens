//! SQLite-backed storage and query layer for the symbol graph. Same schema
//! for every language (a `language` column on `files`, not per-language
//! tables) — this crate knows about SQLite and git, never about any specific
//! language's grammar.

mod error;
mod exclude;
mod indexer;
mod manifests;
mod queries;
mod schema;
mod traversal;

pub use error::{IndexError, Result};
pub use exclude::ExcludeSet;
pub use indexer::{
    DependencyInfo, IndexStatus, LanguageCoverage, ManifestDependencies, ReindexReport,
    UnsupportedFile,
};
pub use queries::{QueryScope, RelationHit, SymbolHit, SymbolListEntry, SymbolMatchMode};

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
    pub fn reindex(&mut self, registry: &LanguageRegistry, force: bool) -> Result<ReindexReport> {
        indexer::reindex(self, registry, force)
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
    /// Relations are resolved by global name, so without a scope a query for
    /// a common name (`main`, `new`, `run`) returns cross-language false
    /// positives (§B1 of `investigacion.md`).
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
    /// [`mct_core::MAX_QUERY_DEPTH`] is clamped.
    pub fn find_calls_bfs(
        &self,
        function: &str,
        depth: u32,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<RelationHit>> {
        self.find_calls_bfs_scoped(function, depth, limit, offset, QueryScope::default())
    }

    /// [`Index::find_calls_bfs`] narrowed to `scope`, applied at *every* hop:
    /// a hit whose referring symbol lies outside the scope is neither
    /// reported nor expanded, so the walk never leaves the scope.
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

    /// [`Index::find_callers_bfs`] narrowed to `scope`, applied at every hop —
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

    /// [`Index::find_references_bfs`] narrowed to `scope`, applied at every
    /// hop — see [`Index::find_calls_bfs_scoped`].
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
}
