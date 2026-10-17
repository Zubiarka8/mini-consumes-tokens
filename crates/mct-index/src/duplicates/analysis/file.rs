//! Loading one indexed file for the analysis: a path that cannot leave the
//! project root, a bounded read, a check that the content is still the
//! content that was indexed, and one tokenization shared by every candidate
//! of the file.

use std::io;
use std::path::{Component, Path, PathBuf};

use git2::{ObjectType, Oid};
use mct_core::{CallableSpan, LineRange, Token, TokenSource};

use crate::{read_repository_file, SkipReason};

/// A file that was read, matched its indexed hash and was tokenized.
pub(super) struct LoadedFile {
    pub(super) source: String,
    /// Sorted by `(line, start_byte)`, so a candidate's tokens are one
    /// contiguous run found by binary search.
    pub(super) tokens: Vec<Token>,
    pub(super) error_lines: Vec<LineRange>,
    pub(super) callables: Option<Vec<CallableSpan>>,
}

/// Why a whole file could not be analyzed; every candidate of the file is
/// skipped with it. The detail never contains file contents.
pub(super) struct FileFailure {
    pub(super) reason: SkipReason,
    pub(super) detail: String,
}

impl FileFailure {
    fn new(reason: SkipReason, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }
}

/// Reads `relative_path` under `root` and tokenizes it, once.
///
/// Order of checks, which decides the skip reason:
/// 1. the path must stay inside `root` ([`SkipReason::ReadError`] otherwise);
/// 2. a missing file is [`SkipReason::StaleLocation`], any other read failure
///    (a directory in its place, permissions, over the size limit) is
///    [`SkipReason::ReadError`];
/// 3. a content hash different from the indexed one is
///    [`SkipReason::StaleLocation`], so a changed file is never read as if it
///    were the indexed one, whatever its encoding;
/// 4. content that is not UTF-8 is [`SkipReason::ReadError`] (the indexer
///    never indexes such a file, so this is only reachable if the stored hash
///    was written by something else);
/// 5. a token source failure is [`SkipReason::ParseError`].
pub(super) fn load(
    root: &Path,
    relative_path: &str,
    indexed_hash: Option<&str>,
    token_source: &dyn TokenSource,
    structural: bool,
) -> Result<LoadedFile, FileFailure> {
    let source = read_source(root, relative_path, indexed_hash)?;
    let tokenized = if structural {
        token_source.tokenize_structural(relative_path, &source)
    } else {
        token_source.tokenize(relative_path, &source)
    }
    .map_err(|error| FileFailure::new(SkipReason::ParseError, error.to_string()))?;

    let mut callables = tokenized.callables;
    if let Some(spans) = &mut callables {
        spans.sort_by_key(|span| span.lines.start);
    }
    let mut tokens = tokenized.tokens;
    tokens.sort_by_key(|token| (token.line, token.start_byte));
    Ok(LoadedFile {
        source,
        tokens,
        error_lines: tokenized.error_lines,
        callables,
    })
}

/// Reads and verifies the same content used during candidate analysis.
pub(super) fn read_source(
    root: &Path,
    relative_path: &str,
    indexed_hash: Option<&str>,
) -> Result<String, FileFailure> {
    let absolute = resolve(root, relative_path)?;
    let bytes = read_repository_file(&absolute).map_err(|error| read_failure(&error))?;

    let Some(indexed_hash) = indexed_hash else {
        return Err(FileFailure::new(
            SkipReason::StaleLocation,
            "the index holds no content hash for this file",
        ));
    };
    let current_hash = Oid::hash_object(ObjectType::Blob, &bytes)
        .map_err(|_| FileFailure::new(SkipReason::ReadError, "could not hash the file content"))?;
    if current_hash.to_string() != indexed_hash {
        return Err(FileFailure::new(
            SkipReason::StaleLocation,
            "content changed since the file was indexed",
        ));
    }

    let source = String::from_utf8(bytes)
        .map_err(|_| FileFailure::new(SkipReason::ReadError, "file content is not valid UTF-8"))?;
    Ok(source)
}

/// Joins a stored relative path onto `root` without ever leaving it: only
/// plain components are accepted, and the resolved path (symlinks followed)
/// must still start with `root`.
fn resolve(root: &Path, relative_path: &str) -> Result<PathBuf, FileFailure> {
    let escapes = || {
        FileFailure::new(
            SkipReason::ReadError,
            "stored path resolves outside the project root",
        )
    };
    let relative = Path::new(relative_path);
    if relative_path.is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(escapes());
    }
    let canonical = root
        .join(relative)
        .canonicalize()
        .map_err(|error| read_failure(&error))?;
    if canonical.starts_with(root) {
        Ok(canonical)
    } else {
        Err(escapes())
    }
}

fn read_failure(error: &io::Error) -> FileFailure {
    if error.kind() == io::ErrorKind::NotFound {
        FileFailure::new(
            SkipReason::StaleLocation,
            "file no longer exists since it was indexed",
        )
    } else {
        FileFailure::new(SkipReason::ReadError, error.to_string())
    }
}
