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
pub use queries::{RelationHit, SymbolHit, SymbolListEntry};

use std::path::{Path, PathBuf};

use ccm_core::LanguageRegistry;
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

/// One open connection to a project's `.ccm-index/index.sqlite3`.
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
        queries::find_symbol(&self.conn, name)
    }

    /// Every place `symbol` is referenced: calls, imports, extends/implements,
    /// and plain references — a superset of [`Index::find_calls`].
    pub fn find_references(&self, symbol: &str) -> Result<Vec<RelationHit>> {
        queries::find_references(&self.conn, symbol)
    }

    /// Calls made *by* `function` (its callees).
    pub fn find_calls(&self, function: &str) -> Result<Vec<RelationHit>> {
        queries::find_calls(&self.conn, function)
    }

    /// Calls made *of* `function` (its callers) — the inverse of [`Index::find_calls`].
    pub fn find_callers(&self, function: &str) -> Result<Vec<RelationHit>> {
        queries::find_callers(&self.conn, function)
    }

    /// Multi-hop [`Index::find_calls`]: walks the call graph forward up to
    /// `depth` hops (1 = identical to [`Index::find_calls`] — the full,
    /// uncapped direct-hit list, so a caller that always passes `depth: 1`
    /// sees the exact same total it always has). Beyond depth 1, stops once
    /// `limit + offset` hits are collected, since an unbounded multi-hop
    /// walk has no equivalent "true total" to preserve. `depth` beyond
    /// [`ccm_core::MAX_QUERY_DEPTH`] is clamped.
    pub fn find_calls_bfs(
        &self,
        function: &str,
        depth: u32,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<RelationHit>> {
        traversal::find_calls_bfs(self, function, depth, bfs_budget(depth, limit, offset))
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
        traversal::find_callers_bfs(self, function, depth, bfs_budget(depth, limit, offset))
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
        traversal::find_references_bfs(self, symbol, depth, bfs_budget(depth, limit, offset))
    }

    /// Lists symbol definitions under `path` (a single file or a
    /// directory/crate prefix), optionally filtered by `kind` and/or
    /// `language`. See [`queries::list_symbols`] for the exact matching
    /// rules.
    pub fn list_symbols(
        &self,
        path: &str,
        kind: Option<&str>,
        language: Option<&str>,
    ) -> Result<Vec<SymbolListEntry>> {
        queries::list_symbols(&self.conn, path, kind, language)
    }
}
