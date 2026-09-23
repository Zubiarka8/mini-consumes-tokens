/// Errors from `mct-index`, kept distinct by cause so callers (and the
/// `get_indexing_status` MCP tool) can tell a database problem apart from a
/// per-file parse failure — they need different remedies.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),

    #[error("git error: {0}")]
    Git(#[from] git2::Error),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid path: {0} escapes the project root")]
    PathEscapesRoot(String),

    #[error("`{0}` is not a directory")]
    NotADirectory(String),
}

pub type Result<T> = std::result::Result<T, IndexError>;
