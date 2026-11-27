//! Writing the path catalog during a reindex: one `path_catalog` row per
//! walked file, reclassified only when its size, modification time or the
//! classification rules changed.

use std::fs::{File, Metadata};
use std::path::Path;
use std::time::UNIX_EPOCH;

use mct_core::LanguageRegistry;
use rusqlite::{params, Connection, OptionalExtension};

use super::classify::{self, classify, needs_signature};
use crate::Result;

/// Bumped whenever the format registry or the classifier changes meaning,
/// so every stored row is reclassified once.
pub const CATALOG_RULES_VERSION: u32 = 1;

/// The rules a row was classified under: [`CATALOG_RULES_VERSION`] plus a
/// fingerprint of which extensions the registry parses (a new language
/// turns `other` rows into `source`). FNV-1a, so it is stable across
/// builds and platforms.
pub fn rules_fingerprint(registry: &LanguageRegistry) -> String {
    let mut claimed: Vec<(&str, &str)> = registry
        .supported_extensions()
        .map(|ext| {
            let language = registry
                .for_extension(ext)
                .map_or("", |parser| parser.language_id());
            (ext, language)
        })
        .collect();
    claimed.sort_unstable();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (ext, language) in claimed {
        for byte in ext.bytes().chain([0]).chain(language.bytes()).chain([0]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("v{CATALOG_RULES_VERSION}:{hash:016x}")
}

/// The language id of the parser the indexer would run on `relative_path`,
/// chosen exactly as `index_source` chooses it.
pub fn parser_for<'a>(registry: &'a LanguageRegistry, relative_path: &str) -> Option<&'a str> {
    let extension = relative_path.rsplit('.').next().unwrap_or_default();
    registry
        .for_extension(extension)
        .map(|parser| parser.language_id())
}

fn modified_ns(metadata: &Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_nanos()).ok())
        .unwrap_or(0)
}

/// Records `relative_path` (the canonical file at `canonical`) in the
/// catalog. A row whose size, modification time and `rules` are unchanged
/// is left alone, so an unchanged tree writes nothing. Only a conflicting
/// suffix opens the file, for at most [`classify::SIGNATURE_BYTES`]; an
/// unreadable prefix classifies as "no evidence" rather than failing.
pub fn record(
    conn: &Connection,
    canonical: &Path,
    relative_path: &str,
    metadata: &Metadata,
    parser: Option<&str>,
    rules: &str,
) -> Result<()> {
    let size = i64::try_from(metadata.len()).ok();
    let modified = modified_ns(metadata);
    let stored: Option<(Option<i64>, i64, String)> = conn
        .prepare_cached(
            "SELECT size_bytes, modified_ns, rules FROM path_catalog WHERE relative_path = ?1",
        )?
        .query_row(params![relative_path], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .optional()?;
    if stored.as_ref() == Some(&(size, modified, rules.to_string())) {
        return Ok(());
    }
    let file_name = relative_path.rsplit('/').next().unwrap_or(relative_path);
    let signature = needs_signature(file_name)
        .then(|| {
            File::open(canonical)
                .and_then(classify::read_signature)
                .ok()
        })
        .flatten();
    let class = classify(file_name, parser, signature.as_deref());
    conn.prepare_cached(
        "INSERT INTO path_catalog
             (relative_path, category, format, evidence, note, size_bytes, modified_ns, rules)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(relative_path) DO UPDATE SET
             category = excluded.category, format = excluded.format,
             evidence = excluded.evidence, note = excluded.note,
             size_bytes = excluded.size_bytes, modified_ns = excluded.modified_ns,
             rules = excluded.rules",
    )?
    .execute(params![
        relative_path,
        class.category.as_str(),
        class.format,
        class.evidence.as_str(),
        class.note,
        size,
        modified,
        rules,
    ])?;
    Ok(())
}
