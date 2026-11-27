//! Path catalog (issue #629): every walked, non-excluded file with its
//! category, canonical format and the evidence for them, so an agent finds
//! a logo, a PDF or a font by path, name, category or format without
//! walking the tree or reading the file.

pub mod classify;
pub mod formats;
pub mod query;
pub(crate) mod store;

use rusqlite::{params, OptionalExtension};

pub use classify::{classify, Classification, Evidence};
pub use formats::{canonical_format, Category};
pub use store::CATALOG_RULES_VERSION;

use crate::{Index, Result};

/// One catalogued file, as a query returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub relative_path: String,
    pub category: String,
    /// Canonical format id, the language id for `source`, or empty.
    pub format: String,
    pub evidence: String,
    /// Why a doubtful classification is what it is.
    pub note: Option<String>,
    pub size_bytes: Option<u64>,
}

const ENTRY_COLUMNS: &str = "relative_path, category, format, evidence, note, size_bytes";

fn entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogEntry> {
    Ok(CatalogEntry {
        relative_path: row.get(0)?,
        category: row.get(1)?,
        format: row.get(2)?,
        evidence: row.get(3)?,
        note: row.get(4)?,
        size_bytes: row
            .get::<_, Option<i64>>(5)?
            .and_then(|n| u64::try_from(n).ok()),
    })
}

impl Index {
    /// The catalog row for exactly `relative_path`, if it was walked.
    pub fn catalog_entry(&self, relative_path: &str) -> Result<Option<CatalogEntry>> {
        Ok(self
            .conn
            .prepare_cached(&format!(
                "SELECT {ENTRY_COLUMNS} FROM path_catalog WHERE relative_path = ?1"
            ))?
            .query_row(params![relative_path], entry)
            .optional()?)
    }
}
