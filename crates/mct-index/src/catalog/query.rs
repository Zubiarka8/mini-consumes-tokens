//! The one catalog query `find_files` and `mct-cli files` share: filters
//! validated and normalized once, rows in path order, paged by offset.

use rusqlite::params_from_iter;

use super::formats::{canonical_format, Category};
use super::{entry, CatalogEntry, ENTRY_COLUMNS};
use crate::{Index, IndexError, Result};

/// Rows a page returns when the caller names no limit.
pub const DEFAULT_FILE_LIMIT: usize = 50;
/// Largest page: bounds one response whatever the caller asks for.
pub const MAX_FILE_LIMIT: usize = 200;

/// Caller input, before validation. Empty strings count as absent.
#[derive(Debug, Clone, Default)]
pub struct FileQueryInput<'a> {
    /// Words that must all occur in the relative path (name included),
    /// case-insensitively.
    pub query: Option<&'a str>,
    /// A file or directory: the path itself or anything under it.
    pub path: Option<&'a str>,
    pub category: Option<&'a str>,
    /// A format id or alias (`jpg`, `.JPG`, `jpeg`), or a parser language
    /// id for `source` files.
    pub format: Option<&'a str>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Why a query was rejected; the message names the valid values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileQueryError {
    UnknownCategory(String),
    UnknownFormat(String),
    InvalidLimit(usize),
    InvalidPath(String),
}

impl std::fmt::Display for FileQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileQueryError::UnknownCategory(name) => {
                let valid: Vec<&str> = Category::ALL.iter().map(|c| c.as_str()).collect();
                write!(
                    f,
                    "unknown category `{name}`; use one of: {}",
                    valid.join(", ")
                )
            }
            FileQueryError::UnknownFormat(name) => write!(
                f,
                "unknown format `{name}`; use a format id or extension alias (pdf, jpg, tar.gz), \
                 a language id of a catalogued source file, or filter by category instead"
            ),
            FileQueryError::InvalidLimit(limit) => {
                write!(f, "limit {limit} is out of range 1..={MAX_FILE_LIMIT}")
            }
            FileQueryError::InvalidPath(path) => write!(
                f,
                "path `{path}` must be relative to the project root, with `/` separators"
            ),
        }
    }
}

impl std::error::Error for FileQueryError {}

/// A validated query: what the rows are filtered and paged by.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileQuery {
    /// Lowercase words, all required.
    terms: Vec<String>,
    /// Normalized: no `./` prefix, no trailing `/`; `None` for the root.
    path: Option<String>,
    category: Option<Category>,
    /// Canonical format id or language id.
    format: Option<String>,
    limit: usize,
    offset: usize,
}

/// One page of matching files, in path order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePage {
    pub entries: Vec<CatalogEntry>,
    /// Every match, across all pages.
    pub total: usize,
    pub offset: usize,
    /// Offset of the next page, `None` on the last one.
    pub next_offset: Option<usize>,
    /// Rows in the whole catalog: 0 means it was never filled.
    pub catalogued: usize,
}

fn present(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

fn normalized_path(path: &str) -> std::result::Result<Option<String>, FileQueryError> {
    let trimmed = path.trim_start_matches("./").trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        return Ok(None);
    }
    if trimmed.starts_with('/')
        || trimmed.contains('\\')
        || trimmed.split('/').any(|part| part == "..")
    {
        return Err(FileQueryError::InvalidPath(path.to_string()));
    }
    Ok(Some(trimmed.to_string()))
}

fn invalid(err: FileQueryError) -> Result<FileQuery> {
    Err(IndexError::InvalidFileQuery(err))
}

impl Index {
    /// Validates `input` against the format registry and, for a format the
    /// registry does not know, against the formats actually catalogued
    /// (parser language ids).
    fn file_query(&self, input: &FileQueryInput<'_>) -> Result<FileQuery> {
        let category = match present(input.category) {
            Some(name) => match Category::parse(name) {
                Some(category) => Some(category),
                None => return invalid(FileQueryError::UnknownCategory(name.to_string())),
            },
            None => None,
        };
        let format = match present(input.format) {
            Some(alias) => match canonical_format(alias) {
                Some(id) => Some(id.to_string()),
                None => {
                    let raw = alias.trim_start_matches('.').to_lowercase();
                    let stored: bool = self.conn.query_row(
                        "SELECT EXISTS(SELECT 1 FROM path_catalog WHERE format = ?1)",
                        [&raw],
                        |row| row.get(0),
                    )?;
                    if !stored {
                        return invalid(FileQueryError::UnknownFormat(alias.to_string()));
                    }
                    Some(raw)
                }
            },
            None => None,
        };
        let path = match present(input.path).map(normalized_path).transpose() {
            Ok(path) => path.flatten(),
            Err(err) => return invalid(err),
        };
        let limit = input.limit.unwrap_or(DEFAULT_FILE_LIMIT);
        if !(1..=MAX_FILE_LIMIT).contains(&limit) {
            return invalid(FileQueryError::InvalidLimit(limit));
        }
        let terms = present(input.query)
            .map(|q| q.split_whitespace().map(str::to_lowercase).collect())
            .unwrap_or_default();
        Ok(FileQuery {
            terms,
            path,
            category,
            format,
            limit,
            offset: input.offset.unwrap_or(0),
        })
    }

    /// The page of catalogued files `input` selects, ordered by relative
    /// path (byte order, as stored), so consecutive pages never repeat or
    /// skip a row of an unchanged index. Every filter is optional and they
    /// combine with AND. Invalid input is
    /// [`IndexError::InvalidFileQuery`].
    pub fn find_files(&self, input: &FileQueryInput<'_>) -> Result<FilePage> {
        let query = self.file_query(input)?;
        let mut sql = format!("SELECT {ENTRY_COLUMNS} FROM path_catalog WHERE 1 = 1");
        let mut bound: Vec<String> = Vec::new();
        if let Some(path) = &query.path {
            // `dir/` ≤ path < `dir0`: everything under `dir/`, as an index
            // range (see `remove_under_prefix`).
            sql.push_str(
                " AND (relative_path = ?1 OR (relative_path >= ?2 AND relative_path < ?3))",
            );
            bound.extend([path.clone(), format!("{path}/"), format!("{path}0")]);
        }
        if let Some(category) = query.category {
            bound.push(category.as_str().to_string());
            sql.push_str(&format!(" AND category = ?{}", bound.len()));
        }
        if let Some(format) = &query.format {
            bound.push(format.clone());
            sql.push_str(&format!(" AND format = ?{}", bound.len()));
        }
        sql.push_str(" ORDER BY relative_path");
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params_from_iter(&bound), entry)?;
        let mut matches: Vec<CatalogEntry> = Vec::new();
        for row in rows {
            let row = row?;
            if !query.terms.is_empty() {
                let folded = row.relative_path.to_lowercase();
                if !query
                    .terms
                    .iter()
                    .all(|term| folded.contains(term.as_str()))
                {
                    continue;
                }
            }
            matches.push(row);
        }
        let total = matches.len();
        let entries: Vec<CatalogEntry> = matches
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .collect();
        let next = query.offset.saturating_add(entries.len());
        let catalogued: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM path_catalog", [], |row| row.get(0))?;
        Ok(FilePage {
            entries,
            total,
            offset: query.offset,
            next_offset: (next < total).then_some(next),
            catalogued: usize::try_from(catalogued).unwrap_or_default(),
        })
    }
}
