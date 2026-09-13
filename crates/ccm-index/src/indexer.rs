use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ccm_core::{LanguageRegistry, ParseError, SourceFile};
use git2::{ObjectType, Oid};
use rusqlite::{params, OptionalExtension};
use walkdir::WalkDir;

use crate::{Index, Result};

/// Extensions of languages this project targets eventually but has no
/// `LanguageParser` for yet, purely so `get_indexing_status` can name a
/// pending language rather than silently ignoring its files (unlike generic
/// non-source files — docs, images, lockfiles — which are ignored without
/// comment).
const KNOWN_PENDING_LANGUAGES: &[(&str, &str)] = &[
    ("java", "java"),
    ("kt", "kotlin"),
    ("kts", "kotlin"),
    ("cs", "csharp"),
    ("cpp", "cpp"),
    ("cc", "cpp"),
    ("cxx", "cpp"),
    ("hpp", "cpp"),
    ("hh", "cpp"),
    ("h", "cpp"),
    ("go", "go"),
    ("swift", "swift"),
    ("php", "php"),
    ("rb", "ruby"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedKind {
    /// A supported target language, but no `LanguageParser` is registered
    /// for it yet (e.g. Go, before `ccm-lang-go` exists).
    UnsupportedLanguage,
    /// A registered parser rejected this file's contents.
    SyntaxError,
}

#[derive(Debug, Clone)]
pub struct UnsupportedFile {
    pub relative_path: String,
    pub kind: UnsupportedKind,
    pub detail: String,
}

#[derive(Debug, Default)]
pub struct ReindexReport {
    pub files_parsed: usize,
    pub files_unchanged: usize,
    pub files_removed: usize,
    pub symbols_written: usize,
    pub issues: Vec<UnsupportedFile>,
}

#[derive(Debug, Clone)]
pub struct LanguageCoverage {
    pub language: String,
    pub file_count: usize,
    pub symbol_count: usize,
}

#[derive(Debug)]
pub struct IndexStatus {
    pub languages: Vec<LanguageCoverage>,
    pub total_files: usize,
    pub total_symbols: usize,
    pub last_indexed_at: Option<i64>,
    /// Target languages seen in the repo with no plugin registered yet.
    pub unsupported_languages: Vec<String>,
    pub syntax_errors: Vec<UnsupportedFile>,
}

pub fn reindex(index: &mut Index, registry: &LanguageRegistry, force: bool) -> Result<ReindexReport> {
    let mut report = ReindexReport::default();
    let root = index.root.clone();

    let mut seen_paths = Vec::new();

    for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();

        // Reject any entry (symlink or not) that resolves outside the project
        // root — never index or follow a symlink that escapes it.
        let canonical = match path.canonicalize() {
            Ok(c) => c,
            Err(_) => continue, // broken symlink or race with a deleted file
        };
        if !canonical.starts_with(&root) {
            continue;
        }

        let relative_path = match to_relative_slash_path(&root, &canonical) {
            Some(p) => p,
            None => continue,
        };
        if index.exclude.is_excluded(&relative_path) {
            continue;
        }
        // Tracked for cleanup below regardless of whether we can index this
        // file, so a stale `files`/`index_issues` row is removed once the
        // file is deleted or excluded, not just once it's re-parsed.
        seen_paths.push(relative_path.clone());

        let extension = relative_path.rsplit('.').next().unwrap_or_default();
        if registry.for_extension(extension).is_none() {
            if let Some((_, language)) = KNOWN_PENDING_LANGUAGES
                .iter()
                .find(|(ext, _)| *ext == extension)
            {
                record_issue(
                    &index.conn,
                    &relative_path,
                    UnsupportedKind::UnsupportedLanguage,
                    language,
                )?;
                report.issues.push(UnsupportedFile {
                    relative_path: relative_path.clone(),
                    kind: UnsupportedKind::UnsupportedLanguage,
                    detail: (*language).to_string(),
                });
            }
            continue;
        }

        let bytes = match std::fs::read(&canonical) {
            Ok(b) => b,
            Err(_) => continue, // unreadable (permissions, race) — skip, don't fail the run
        };
        let content_hash = match Oid::hash_object(ObjectType::Blob, &bytes) {
            Ok(oid) => oid.to_string(),
            Err(_) => continue,
        };

        let existing_hash: Option<String> = index
            .conn
            .query_row(
                "SELECT content_hash FROM files WHERE relative_path = ?1",
                params![relative_path],
                |row| row.get(0),
            )
            .optional()?;

        if !force && existing_hash.as_deref() == Some(content_hash.as_str()) {
            report.files_unchanged += 1;
            continue;
        }

        let contents = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => continue, // binary file with a matching extension — nothing to parse
        };

        let source = SourceFile {
            relative_path: relative_path.clone(),
            contents,
        };

        match registry.parse(&source) {
            Ok(parsed) => {
                let language = registry
                    .for_extension(extension)
                    .expect("checked above")
                    .language_id();
                let symbols_written = write_parsed_file(
                    index,
                    &relative_path,
                    language,
                    &content_hash,
                    &parsed,
                )?;
                report.files_parsed += 1;
                report.symbols_written += symbols_written;
            }
            Err(ParseError::Syntax { line, message, .. }) => {
                let detail = format!("line {line}: {message}");
                record_issue(
                    &index.conn,
                    &relative_path,
                    UnsupportedKind::SyntaxError,
                    &detail,
                )?;
                report.issues.push(UnsupportedFile {
                    relative_path: relative_path.clone(),
                    kind: UnsupportedKind::SyntaxError,
                    detail,
                });
            }
            Err(ParseError::UnsupportedExtension { .. }) => {
                // Registry already confirmed a parser exists for this
                // extension above; unreachable in practice.
            }
        }
    }

    report.files_removed = remove_missing_files(index, &seen_paths)?;
    touch_last_indexed_at(index)?;

    Ok(report)
}

fn write_parsed_file(
    index: &mut Index,
    relative_path: &str,
    language: &str,
    content_hash: &str,
    parsed: &ccm_core::ParsedFile,
) -> Result<usize> {
    let now = unix_now();
    let tx = index.conn.transaction()?;

    let file_id: i64 = {
        tx.execute(
            "INSERT INTO files (relative_path, language, content_hash, last_indexed_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(relative_path) DO UPDATE SET
                language = excluded.language,
                content_hash = excluded.content_hash,
                last_indexed_at = excluded.last_indexed_at",
            params![relative_path, language, content_hash, now],
        )?;
        tx.query_row(
            "SELECT id FROM files WHERE relative_path = ?1",
            params![relative_path],
            |row| row.get(0),
        )?
    };

    tx.execute("DELETE FROM symbols WHERE file_id = ?1", params![file_id])?;
    // Also clear stale index_issues entries for this path (e.g. it used to
    // fail to parse and now succeeds).
    tx.execute(
        "DELETE FROM index_issues WHERE relative_path = ?1",
        params![relative_path],
    )?;

    // Local symbol id -> database row id, to resolve relations below.
    let mut id_map: HashMap<u32, i64> = HashMap::with_capacity(parsed.symbols.len());
    for symbol in &parsed.symbols {
        tx.execute(
            "INSERT INTO symbols (file_id, name, kind, parent, line, column, byte_len)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                file_id,
                symbol.name,
                symbol_kind_str(symbol.kind),
                symbol.parent,
                symbol.location.line,
                symbol.location.column,
                symbol.location.byte_len,
            ],
        )?;
        id_map.insert(symbol.id, tx.last_insert_rowid());
    }

    for relation in &parsed.relations {
        let Some(&from_row_id) = id_map.get(&relation.from) else {
            continue; // parser bug guard: relation referencing an unknown local symbol id
        };
        // Best-effort resolution to another symbol by name, anywhere in the
        // index (same file or not); left NULL when not found yet — later
        // reindexing of the defining file will not retroactively backfill
        // this row, so `find_references`/`find_calls` also match by name.
        let to_symbol_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM symbols WHERE name = ?1 LIMIT 1",
                params![relation.to_name],
                |row| row.get(0),
            )
            .optional()?;
        tx.execute(
            "INSERT INTO relations (from_symbol_id, kind, to_name, to_symbol_id, line, column, byte_len)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                from_row_id,
                relation_kind_str(relation.kind),
                relation.to_name,
                to_symbol_id,
                relation.location.line,
                relation.location.column,
                relation.location.byte_len,
            ],
        )?;
    }

    let count = parsed.symbols.len();
    tx.commit()?;
    Ok(count)
}

fn remove_missing_files(index: &mut Index, seen_paths: &[String]) -> Result<usize> {
    let tx = index.conn.transaction()?;
    let seen: std::collections::HashSet<&str> = seen_paths.iter().map(String::as_str).collect();

    let mut stmt = tx.prepare("SELECT relative_path FROM files")?;
    let stored: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    let mut removed = 0;
    for path in &stored {
        if !seen.contains(path.as_str()) {
            tx.execute("DELETE FROM files WHERE relative_path = ?1", params![path])?;
            removed += 1;
        }
    }

    // index_issues rows exist independently of `files` (an unsupported-
    // language or syntax-error file never gets a `files` row at all), so
    // sweep them separately for anything no longer seen on disk.
    let mut stmt = tx.prepare("SELECT DISTINCT relative_path FROM index_issues")?;
    let issue_paths: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    for path in issue_paths {
        if !seen.contains(path.as_str()) {
            tx.execute(
                "DELETE FROM index_issues WHERE relative_path = ?1",
                params![path],
            )?;
        }
    }

    tx.commit()?;
    Ok(removed)
}

fn record_issue(
    conn: &rusqlite::Connection,
    relative_path: &str,
    kind: UnsupportedKind,
    detail: &str,
) -> Result<()> {
    let issue_kind = match kind {
        UnsupportedKind::UnsupportedLanguage => "unsupported_language",
        UnsupportedKind::SyntaxError => "syntax_error",
    };
    conn.execute(
        "INSERT INTO index_issues (relative_path, issue_kind, detail, detected_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(relative_path, issue_kind) DO UPDATE SET
            detail = excluded.detail,
            detected_at = excluded.detected_at",
        params![relative_path, issue_kind, detail, unix_now()],
    )?;
    Ok(())
}

fn touch_last_indexed_at(index: &mut Index) -> Result<()> {
    let now = unix_now().to_string();
    index.conn.execute(
        "INSERT INTO index_meta (key, value) VALUES ('last_indexed_at', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![now],
    )?;
    Ok(())
}

pub fn status(index: &Index) -> Result<IndexStatus> {
    let mut stmt = index.conn.prepare(
        "SELECT f.language, COUNT(DISTINCT f.id), COUNT(s.id)
         FROM files f LEFT JOIN symbols s ON s.file_id = f.id
         GROUP BY f.language ORDER BY f.language",
    )?;
    let languages: Vec<LanguageCoverage> = stmt
        .query_map([], |row| {
            Ok(LanguageCoverage {
                language: row.get(0)?,
                file_count: row.get::<_, i64>(1)? as usize,
                symbol_count: row.get::<_, i64>(2)? as usize,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    let total_files: i64 =
        index
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
    let total_symbols: i64 =
        index
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;
    let last_indexed_at: Option<String> = index
        .conn
        .query_row(
            "SELECT value FROM index_meta WHERE key = 'last_indexed_at'",
            [],
            |row| row.get(0),
        )
        .optional()?;

    let mut stmt = index.conn.prepare(
        "SELECT DISTINCT detail FROM index_issues WHERE issue_kind = 'unsupported_language'",
    )?;
    let unsupported_languages: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    let mut stmt = index.conn.prepare(
        "SELECT relative_path, detail FROM index_issues WHERE issue_kind = 'syntax_error'",
    )?;
    let syntax_errors: Vec<UnsupportedFile> = stmt
        .query_map([], |row| {
            Ok(UnsupportedFile {
                relative_path: row.get(0)?,
                kind: UnsupportedKind::SyntaxError,
                detail: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    Ok(IndexStatus {
        languages,
        total_files: total_files as usize,
        total_symbols: total_symbols as usize,
        last_indexed_at: last_indexed_at.and_then(|v| v.parse().ok()),
        unsupported_languages,
        syntax_errors,
    })
}

fn symbol_kind_str(kind: ccm_core::SymbolKind) -> &'static str {
    use ccm_core::SymbolKind::*;
    match kind {
        Function => "function",
        Method => "method",
        Class => "class",
        Struct => "struct",
        Interface => "interface",
        Enum => "enum",
        Trait => "trait",
        TypeAlias => "type_alias",
        Module => "module",
        Variable => "variable",
        Constant => "constant",
        Field => "field",
    }
}

fn relation_kind_str(kind: ccm_core::RelationKind) -> &'static str {
    use ccm_core::RelationKind::*;
    match kind {
        Calls => "calls",
        Imports => "imports",
        Extends => "extends",
        Implements => "implements",
        References => "references",
    }
}

fn to_relative_slash_path(root: &Path, canonical: &Path) -> Option<String> {
    let rel = canonical.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for component in rel.components() {
        parts.push(component.as_os_str().to_str()?.to_string());
    }
    Some(parts.join("/"))
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
