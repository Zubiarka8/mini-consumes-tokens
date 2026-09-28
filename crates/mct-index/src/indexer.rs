use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use git2::{ObjectType, Oid};
use mct_core::{LanguageRegistry, ParseError, SourceFile};
use rusqlite::{params, OptionalExtension};
use walkdir::WalkDir;

use crate::manifests;
use crate::{ExcludeSet, Index, Result};

/// Extensions of languages this project targets eventually but has no
/// `LanguageParser` for yet, purely so `get_indexing_status` can name a
/// pending language rather than silently ignoring its files (unlike generic
/// non-source files — docs, images, lockfiles — which are ignored without
/// comment).
const KNOWN_PENDING_LANGUAGES: &[(&str, &str)] = &[("swift", "swift"), ("rb", "ruby")];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedKind {
    /// A supported target language, but no `LanguageParser` is registered
    /// for it yet (e.g. Go, before `mct-lang-go` exists).
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

#[derive(Debug, Clone)]
pub struct DependencyInfo {
    pub name: String,
    /// Absent for a path/git dependency, or one whose real version lives in
    /// a workspace root manifest (`{ workspace = true }`).
    pub version: Option<String>,
}

/// The dependencies declared by one manifest file (`Cargo.toml`,
/// `package.json`, `requirements.txt`, `go.mod`).
#[derive(Debug, Clone)]
pub struct ManifestDependencies {
    pub manifest_path: String,
    pub language: String,
    pub dependencies: Vec<DependencyInfo>,
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
    pub dependencies: Vec<ManifestDependencies>,
}

pub fn reindex(
    index: &mut Index,
    registry: &LanguageRegistry,
    force: bool,
) -> Result<ReindexReport> {
    let mut report = ReindexReport::default();
    let root = index.root.clone();

    let mut seen_paths = Vec::new();
    let mut seen_manifest_paths = Vec::new();

    // Cloned out of `index` so the `filter_entry` closure below doesn't hold a
    // borrow of it across the loop body, which needs `&mut Index` to write.
    let exclude = index.exclude.clone();

    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_entry(|entry| walk_entry_allowed(&root, entry, &exclude))
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_dir() {
            continue;
        }
        match index_file(index, registry, entry.path(), force, &mut report)? {
            Seen::Source(path) => seen_paths.push(path),
            Seen::Manifest(path) => seen_manifest_paths.push(path),
            Seen::Nothing => {}
        }
    }

    report.files_removed = remove_missing_files(index, &seen_paths)?;
    remove_missing_manifests(index, &seen_manifest_paths)?;
    touch_last_indexed_at(index)?;

    Ok(report)
}

/// What [`index_file`] found at a path, for the stale-row sweeps.
enum Seen {
    /// A file that keeps (or gets) its `files`/`index_issues` rows.
    Source(String),
    /// A manifest whose `dependencies` rows were rewritten.
    Manifest(String),
    /// Nothing to keep: outside the root, excluded, or gone.
    Nothing,
}

/// Indexes the one file at `path` (absolute, as walked or as a watcher
/// reported it): the per-file half of [`reindex`], shared with
/// [`reindex_paths`] so both apply identical rules — the escape and exclusion
/// checks on the canonical path, manifests re-parsed every time, source files
/// skipped when their content hash is unchanged (unless `force`).
fn index_file(
    index: &mut Index,
    registry: &LanguageRegistry,
    path: &Path,
    force: bool,
    report: &mut ReindexReport,
) -> Result<Seen> {
    let root = index.root.clone();

    // Reject any entry (symlink or not) that resolves outside the project
    // root — never index or follow a symlink that escapes it. Runs only on
    // entries that already survived the exclusion filter, so the syscall is
    // never paid for `target/`, `.git/` and friends.
    let canonical = match path.canonicalize() {
        Ok(c) => c,
        Err(_) => return Ok(Seen::Nothing), // broken symlink or race with a deleted file
    };
    if !canonical.starts_with(&root) {
        return Ok(Seen::Nothing);
    }

    let relative_path = match to_relative_slash_path(&root, &canonical) {
        Some(p) => p,
        None => return Ok(Seen::Nothing),
    };
    // Second exclusion check, on the *canonical* path: the walk judged the
    // path as written, which differs for a symlink pointing into an excluded
    // directory.
    if index.exclude.is_excluded(&relative_path) {
        return Ok(Seen::Nothing);
    }

    // Manifest files (Cargo.toml, package.json, ...) are matched by
    // file name, not extension, and never go through a `LanguageParser`
    // — they aren't source code to symbol-index, just a declared
    // dependency list. Always re-parsed on every reindex (not
    // hash-skipped like source files below): manifests are few and
    // small, not worth a second incremental-skip mechanism for.
    let file_name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if let Some(language) = manifests::manifest_language(file_name) {
        if let Ok(bytes) = std::fs::read(&canonical) {
            if let Ok(contents) = String::from_utf8(bytes) {
                let deps = manifests::parse_manifest(file_name, &contents);
                write_manifest_dependencies(index, &relative_path, language, &deps)?;
            }
        }
        return Ok(Seen::Manifest(relative_path));
    }

    // Reported as seen regardless of whether we can index this file, so a
    // stale `files`/`index_issues` row is removed once the file is deleted
    // or excluded, not just once it's re-parsed.
    let extension = relative_path.rsplit('.').next().unwrap_or_default();
    let Some(parser) = registry.for_extension(extension) else {
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
        return Ok(Seen::Source(relative_path));
    };

    let bytes = match std::fs::read(&canonical) {
        Ok(b) => b,
        // unreadable (permissions, race) — skip, don't fail the run
        Err(_) => return Ok(Seen::Source(relative_path)),
    };
    let content_hash = match Oid::hash_object(ObjectType::Blob, &bytes) {
        Ok(oid) => oid.to_string(),
        Err(_) => return Ok(Seen::Source(relative_path)),
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
        return Ok(Seen::Source(relative_path));
    }

    let contents = match String::from_utf8(bytes) {
        Ok(s) => s,
        // binary file with a matching extension — nothing to parse
        Err(_) => return Ok(Seen::Source(relative_path)),
    };

    let source = SourceFile {
        relative_path: relative_path.clone(),
        contents,
    };

    match registry.parse(&source) {
        Ok(parsed) => {
            let language = parser.language_id();
            let symbols_written =
                write_parsed_file(index, &relative_path, language, &content_hash, &parsed)?;
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
    Ok(Seen::Source(relative_path))
}

/// Incremental counterpart of [`reindex`] (issue #25): brings the index up to
/// date for `paths` only — absolute paths under the root (or relative to
/// it), typically what a filesystem watcher reported — without walking or
/// hashing the rest of the project. Each path is handled by what is on disk
/// *now*:
///
/// - a file: indexed exactly as [`reindex`] would (hash-skipped if unchanged);
/// - a directory (created, or renamed into place): its subtree is walked, and
///   indexed rows under it that are no longer on disk are dropped;
/// - nothing (deleted, or renamed away): every row for that path — and, if it
///   was a directory, for everything under it — is dropped.
///
/// Then any indexed file no longer on disk is dropped even if unreported
/// (see [`remove_vanished`]): a missed deletion or rename-away can never
/// leave stale symbols. Excluded paths and paths outside the root are
/// ignored; the root itself falls back to a full [`reindex`]. Relations are resolved by name at query
/// time and a symbol's embedding/literals cascade with it, so rewriting one
/// file's rows is all an update needs: the result is the same index a full
/// [`reindex`] would produce.
pub fn reindex_paths(
    index: &mut Index,
    registry: &LanguageRegistry,
    paths: &[PathBuf],
) -> Result<ReindexReport> {
    let mut report = ReindexReport::default();
    let root = index.root.clone();
    let exclude = index.exclude.clone();

    let mut unique: Vec<PathBuf> = paths
        .iter()
        .map(|p| {
            if p.is_absolute() {
                p.clone()
            } else {
                root.join(p)
            }
        })
        .collect();
    unique.sort();
    unique.dedup();

    for absolute in unique {
        // Judged as written, not canonicalized: a deleted path can't be.
        let Some(relative_path) = to_relative_slash_path(&root, &absolute) else {
            continue;
        };
        if relative_path.is_empty() {
            return reindex(index, registry, false);
        }
        if exclude.is_excluded(&relative_path) {
            continue;
        }

        if absolute.is_dir() {
            let mut seen = Vec::new();
            let mut seen_manifests = Vec::new();
            for entry in WalkDir::new(&absolute)
                .into_iter()
                .filter_entry(|entry| {
                    entry.depth() == 0 || walk_entry_allowed(&root, entry, &exclude)
                })
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_dir() {
                    continue;
                }
                match index_file(index, registry, entry.path(), false, &mut report)? {
                    Seen::Source(p) => seen.push(p),
                    Seen::Manifest(p) => seen_manifests.push(p),
                    Seen::Nothing => {}
                }
            }
            report.files_removed +=
                remove_under_prefix(index, &relative_path, &seen, &seen_manifests)?;
        } else if absolute.symlink_metadata().is_ok() {
            if let Seen::Nothing = index_file(index, registry, &absolute, false, &mut report)? {
                // Now outside the root or excluded (a retargeted symlink, a
                // broken one): whatever was indexed under it is stale.
                report.files_removed += remove_under_prefix(index, &relative_path, &[], &[])?;
            }
        } else {
            report.files_removed += remove_under_prefix(index, &relative_path, &[], &[])?;
        }
    }

    report.files_removed += remove_vanished(index)?;
    touch_last_indexed_at(index)?;
    Ok(report)
}

/// Drops the rows of every indexed file, issue or manifest that no longer
/// exists on disk, whether or not its path was reported. Watchers don't
/// reliably report both ends of a rename (macOS FSEvents through the
/// debouncer's file-id cache reports only the new path), and a lost event
/// would otherwise leave a deleted file's symbols behind. One `stat` per
/// indexed path — no read, no hash — so it stays cheap next to a full walk.
fn remove_vanished(index: &mut Index) -> Result<usize> {
    let root = index.root.clone();
    let mut removed = 0;
    for (table, column) in [
        ("files", "relative_path"),
        ("index_issues", "relative_path"),
        ("dependencies", "manifest_path"),
    ] {
        let stored: Vec<String> = {
            let mut stmt = index
                .conn
                .prepare_cached(&format!("SELECT DISTINCT {column} FROM {table}"))?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let vanished: Vec<&String> = stored
            .iter()
            .filter(|path| root.join(path).symlink_metadata().is_err())
            .collect();
        if vanished.is_empty() {
            continue;
        }
        let tx = index.conn.transaction()?;
        for path in vanished {
            let deleted = tx.execute(
                &format!("DELETE FROM {table} WHERE {column} = ?1"),
                params![path],
            )?;
            if table == "files" {
                removed += deleted;
            }
        }
        tx.commit()?;
    }
    Ok(removed)
}

/// Drops every `files`, `index_issues` and `dependencies` row for
/// `relative_path` itself or for anything under it as a directory, except
/// the paths in `keep`/`keep_manifests`. Returns how many `files` rows went.
fn remove_under_prefix(
    index: &mut Index,
    relative_path: &str,
    keep: &[String],
    keep_manifests: &[String],
) -> Result<usize> {
    let keep: HashSet<&str> = keep.iter().map(String::as_str).collect();
    let keep_manifests: HashSet<&str> = keep_manifests.iter().map(String::as_str).collect();
    // `LIKE` would treat `_`/`%` in the path as wildcards; `substr` matches a
    // true `dir/` prefix only. Table/column names are static, never input.
    let prefix = format!("{relative_path}/");
    let tx = index.conn.transaction()?;
    let mut removed = 0;
    for (table, column, keep) in [
        ("files", "relative_path", &keep),
        ("index_issues", "relative_path", &keep),
        ("dependencies", "manifest_path", &keep_manifests),
    ] {
        let stored: Vec<String> = {
            let mut stmt = tx.prepare(&format!(
                "SELECT DISTINCT {column} FROM {table}
                 WHERE {column} = ?1 OR substr({column}, 1, length(?2)) = ?2"
            ))?;
            let rows = stmt.query_map(params![relative_path, prefix], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for path in stored.iter().filter(|p| !keep.contains(p.as_str())) {
            let deleted = tx.execute(
                &format!("DELETE FROM {table} WHERE {column} = ?1"),
                params![path],
            )?;
            if table == "files" {
                removed += deleted;
            }
        }
    }
    tx.commit()?;
    Ok(removed)
}

fn write_manifest_dependencies(
    index: &mut Index,
    manifest_path: &str,
    language: &str,
    deps: &[manifests::ManifestDependency],
) -> Result<()> {
    let tx = index.conn.transaction()?;
    tx.execute(
        "DELETE FROM dependencies WHERE manifest_path = ?1",
        params![manifest_path],
    )?;
    for dep in deps {
        tx.execute(
            "INSERT INTO dependencies (manifest_path, language, name, version) VALUES (?1, ?2, ?3, ?4)",
            params![manifest_path, language, dep.name, dep.version],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn remove_missing_manifests(index: &mut Index, seen_manifest_paths: &[String]) -> Result<()> {
    let tx = index.conn.transaction()?;
    let seen: std::collections::HashSet<&str> =
        seen_manifest_paths.iter().map(String::as_str).collect();

    let mut stmt = tx.prepare("SELECT DISTINCT manifest_path FROM dependencies")?;
    let stored: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    for path in &stored {
        if !seen.contains(path.as_str()) {
            tx.execute(
                "DELETE FROM dependencies WHERE manifest_path = ?1",
                params![path],
            )?;
        }
    }

    tx.commit()?;
    Ok(())
}

fn write_parsed_file(
    index: &mut Index,
    relative_path: &str,
    language: &str,
    content_hash: &str,
    parsed: &mct_core::ParsedFile,
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

    // Literals first: deleting symbols first would null each literal's
    // `symbol_id` one row at a time, only for the rows to be deleted anyway.
    tx.execute("DELETE FROM literals WHERE file_id = ?1", params![file_id])?;
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
            "INSERT INTO symbols (file_id, name, kind, parent, line, column, byte_len, end_line, level)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                file_id,
                symbol.name,
                symbol_kind_str(symbol.kind),
                symbol.parent,
                symbol.location.line,
                symbol.location.column,
                symbol.location.byte_len,
                symbol.location.end_line,
                symbol.level,
            ],
        )?;
        id_map.insert(symbol.id, tx.last_insert_rowid());
    }

    for relation in &parsed.relations {
        let Some(&from_row_id) = id_map.get(&relation.from) else {
            continue; // parser bug guard: relation referencing an unknown local symbol id
        };
        tx.execute(
            "INSERT INTO relations (from_symbol_id, kind, to_name, line, column, byte_len)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                from_row_id,
                relation_kind_str(relation.kind),
                relation.to_name,
                relation.location.line,
                relation.location.column,
                relation.location.byte_len,
            ],
        )?;
    }

    // Capped again here, not only by the parser's `LiteralCollector`, so a
    // parser that builds the Vec by hand can't bloat the index.
    for literal in parsed.literals.iter().take(mct_core::MAX_LITERALS_PER_FILE) {
        let symbol_id = enclosing_symbol(&parsed.symbols, literal.line)
            .and_then(|local| id_map.get(&local).copied());
        tx.execute(
            "INSERT INTO literals (file_id, symbol_id, line, text) VALUES (?1, ?2, ?3, ?4)",
            params![file_id, symbol_id, literal.line, literal.text],
        )?;
    }

    let count = parsed.symbols.len();
    tx.commit()?;
    Ok(count)
}

/// Local id of the innermost symbol whose span holds `line`: the one that
/// starts last, then ends first, then was emitted last — parsers emit an
/// enclosing symbol before what it contains, so on an identical span (a
/// function that fills its whole file, and the file-level module) the inner
/// one wins. A symbol with no `end_line` spans only its first line.
fn enclosing_symbol(symbols: &[mct_core::SymbolRecord], line: u32) -> Option<u32> {
    symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            let start = s.location.line;
            start <= line && line <= s.location.end_line.unwrap_or(start)
        })
        .min_by_key(|(i, s)| {
            (
                std::cmp::Reverse(s.location.line),
                s.location.end_line.unwrap_or(s.location.line),
                std::cmp::Reverse(*i),
            )
        })
        .map(|(_, s)| s.id)
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

    let total_files: i64 = index
        .conn
        .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
    let total_symbols: i64 = index
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

    let mut stmt = index.conn.prepare(
        "SELECT manifest_path, language, name, version FROM dependencies
         ORDER BY manifest_path, name",
    )?;
    let dependency_rows: Vec<(String, String, String, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    let mut dependencies: Vec<ManifestDependencies> = Vec::new();
    for (manifest_path, language, name, version) in dependency_rows {
        match dependencies.last_mut() {
            Some(last) if last.manifest_path == manifest_path => {
                last.dependencies.push(DependencyInfo { name, version });
            }
            _ => dependencies.push(ManifestDependencies {
                manifest_path,
                language,
                dependencies: vec![DependencyInfo { name, version }],
            }),
        }
    }

    Ok(IndexStatus {
        languages,
        total_files: total_files as usize,
        total_symbols: total_symbols as usize,
        last_indexed_at: last_indexed_at.and_then(|v| v.parse().ok()),
        unsupported_languages,
        syntax_errors,
        dependencies,
    })
}

fn symbol_kind_str(kind: mct_core::SymbolKind) -> &'static str {
    use mct_core::SymbolKind::*;
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
        Element => "element",
        Rule => "rule",
    }
}

fn relation_kind_str(kind: mct_core::RelationKind) -> &'static str {
    use mct_core::RelationKind::*;
    match kind {
        Calls => "calls",
        Imports => "imports",
        Extends => "extends",
        Implements => "implements",
        References => "references",
    }
}

/// Exclusion filter for the walk itself. Unlike the per-file check in
/// [`reindex`] this also runs on directories, which is what lets
/// `WalkDir::filter_entry` prune an excluded directory instead of descending
/// into it — in a Rust checkout `target/` and `.git/` alone are tens of
/// thousands of entries that would otherwise be visited (and canonicalized)
/// on every reindex only to be discarded one by one.
///
/// The relative path is derived by stripping the walk root, never by
/// canonicalizing: a `canonicalize()` here would reintroduce the very syscall
/// the pruning exists to avoid. Entries that survive this filter still go
/// through the canonical-path escape check in [`reindex`], which is what
/// actually guards against symlinks resolving outside the root.
fn walk_entry_allowed(root: &Path, entry: &walkdir::DirEntry, exclude: &ExcludeSet) -> bool {
    // Depth 0 is the walk root itself. Filtering it out would abort the walk
    // before it starts — e.g. for a project checked out into a directory
    // that happens to be named `target`.
    if entry.depth() == 0 {
        return true;
    }
    match to_relative_slash_path(root, entry.path()) {
        Some(relative_path) => !exclude.is_excluded(&relative_path),
        // Not expressible as a relative slash path (a non-UTF-8 component, or
        // an entry somehow outside the walk root): don't prune on a guess —
        // the canonical check in `reindex` decides.
        None => true,
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
