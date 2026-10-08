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
    /// The file, manifest or directory could not be read (permissions, a
    /// race): nothing is known about its current content, which says nothing
    /// about its syntax or language. Whatever was last indexed for the path
    /// is kept, and the issue is cleared by the next successful read.
    ReadFailure,
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
    /// Paths that could not be read on the last attempt. Their previous rows
    /// (symbols, dependencies) are still served, so they may be stale: a
    /// status that lists any is not fully healthy.
    pub read_failures: Vec<UnsupportedFile>,
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
    // Paths that could not be read: the sweeps below must not take them (or
    // anything under them) for deleted.
    let mut unreadable: Vec<String> = Vec::new();

    // Pick up edits to the project's ignore files first: the walk below then
    // skips newly excluded paths (and the sweep drops their rows) and indexes
    // newly un-excluded ones, forced or not.
    index.exclude.reload();

    // Cloned out of `index` so the `filter_entry` closure below doesn't hold a
    // borrow of it across the loop body, which needs `&mut Index` to write.
    let exclude = index.exclude.clone();

    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_entry(|entry| walk_entry_allowed(&root, entry, &exclude))
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                unreadable.push(record_walk_failure(index, &err, &mut report)?);
                continue;
            }
        };
        if entry.file_type().is_dir() {
            continue;
        }
        match index_file(index, registry, entry.path(), force, &mut report)? {
            Seen::Source(path) => seen_paths.push(path),
            Seen::Manifest(path) => seen_manifest_paths.push(path),
            Seen::Unreadable(path) => unreadable.push(path),
            Seen::Nothing => {}
        }
    }

    report.files_removed = remove_missing_files(index, &seen_paths, &unreadable)?;
    remove_missing_manifests(index, &seen_manifest_paths, &unreadable)?;
    touch_last_indexed_at(index)?;

    Ok(report)
}

/// What [`index_file`] found at a path, for the stale-row sweeps.
enum Seen {
    /// A file that keeps (or gets) its `files`/`index_issues` rows.
    Source(String),
    /// A manifest whose `dependencies` rows were rewritten.
    Manifest(String),
    /// A path that exists but could not be read (a read failure is on record
    /// for it): every row for it, and for anything under it, is kept as is.
    Unreadable(String),
    /// Nothing to keep: outside the root, excluded, or gone.
    Nothing,
}

/// Whether `path` is `retained` or lies under it as a directory. The empty
/// path is the project root, so it retains everything.
fn is_retained(path: &str, retained: &[String]) -> bool {
    retained.iter().any(|r| {
        r.is_empty()
            || path == r
            || path
                .strip_prefix(r.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Records a read failure for `relative_path`, keeping its previous rows.
fn record_read_failure(
    index: &Index,
    report: &mut ReindexReport,
    relative_path: &str,
    detail: String,
) -> Result<()> {
    record_issue(
        &index.conn,
        relative_path,
        UnsupportedKind::ReadFailure,
        &detail,
    )?;
    report.issues.push(UnsupportedFile {
        relative_path: relative_path.to_string(),
        kind: UnsupportedKind::ReadFailure,
        detail,
    });
    Ok(())
}

/// A directory walk error (an unreadable directory, an entry that vanished
/// or can't be stat'ed) recorded as a read failure at the path it names.
/// Returns that path to retain: the files below it were not visited, which
/// must not read as their deletion. A path the walk can't name (or that isn't
/// under the root) is the root, retaining everything.
fn record_walk_failure(
    index: &Index,
    err: &walkdir::Error,
    report: &mut ReindexReport,
) -> Result<String> {
    let relative_path = err
        .path()
        .and_then(|p| to_relative_slash_path(&index.root, p))
        .unwrap_or_default();
    record_read_failure(index, report, &relative_path, err.to_string())?;
    Ok(relative_path)
}

/// Drops a read failure once its path has been read again.
fn clear_read_failure(index: &Index, relative_path: &str) -> Result<()> {
    index
        .conn
        .prepare_cached(
            "DELETE FROM index_issues WHERE relative_path = ?1 AND issue_kind = 'read_failure'",
        )?
        .execute(params![relative_path])?;
    Ok(())
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
        // Not allowed to resolve it (a parent lost its search permission):
        // it may well still exist, so keep whatever is indexed for it.
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            let Some(relative_path) = to_relative_slash_path(&root, path) else {
                return Ok(Seen::Nothing);
            };
            record_read_failure(index, report, &relative_path, err.to_string())?;
            return Ok(Seen::Unreadable(relative_path));
        }
        Err(_) => return Ok(Seen::Nothing), // broken symlink or race with a deleted file
    };
    // A symlink to a directory resolves to one: nothing to index as a file,
    // and never walked (the walks don't follow links).
    if !canonical.starts_with(&root) || canonical.is_dir() {
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
        match crate::read_repository_file(&canonical) {
            Ok(bytes) => {
                clear_read_failure(index, &relative_path)?;
                if let Ok(contents) = String::from_utf8(bytes) {
                    let deps = manifests::parse_manifest(file_name, &contents);
                    write_manifest_dependencies(index, &relative_path, language, &deps)?;
                }
            }
            // Unreadable (permissions, race): the last-read dependencies stay,
            // flagged as possibly stale, rather than being swept as removed.
            Err(err) => {
                record_read_failure(index, report, &relative_path, err.to_string())?;
                return Ok(Seen::Unreadable(relative_path));
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

    let bytes = match crate::read_repository_file(&canonical) {
        Ok(b) => b,
        // Unreadable (permissions, race): not a syntax error and not an
        // unsupported language, so it is its own issue. The run carries on and
        // the last-good symbols stay, flagged as possibly stale.
        Err(err) => {
            record_read_failure(index, report, &relative_path, err.to_string())?;
            return Ok(Seen::Unreadable(relative_path));
        }
    };
    clear_read_failure(index, &relative_path)?;
    let content_hash = match Oid::hash_object(ObjectType::Blob, &bytes) {
        Ok(oid) => oid.to_string(),
        Err(_) => return Ok(Seen::Source(relative_path)),
    };

    // A syntax error no longer keeps a `files` row (see below), but an index
    // written before that did: the flag lets a hash-skip clear such a stale
    // issue once the file is back to its last parsed content.
    let existing: Option<(String, bool)> = index
        .conn
        .query_row(
            "SELECT content_hash, EXISTS(SELECT 1 FROM index_issues
                 WHERE relative_path = ?1 AND issue_kind = 'syntax_error')
             FROM files WHERE relative_path = ?1",
            params![relative_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    if let Some((hash, stale_issue)) = &existing {
        if !force && *hash == content_hash {
            if *stale_issue {
                index.conn.execute(
                    "DELETE FROM index_issues
                     WHERE relative_path = ?1 AND issue_kind = 'syntax_error'",
                    params![relative_path],
                )?;
            }
            report.files_unchanged += 1;
            return Ok(Seen::Source(relative_path));
        }
    }

    let contents = match String::from_utf8(bytes) {
        Ok(s) => s,
        // binary file with a matching extension — nothing to parse, and
        // nothing from an earlier text version of it may stay indexed
        Err(_) => {
            if existing.is_some() {
                index.conn.execute(
                    "DELETE FROM files WHERE relative_path = ?1",
                    params![relative_path],
                )?;
            }
            return Ok(Seen::Source(relative_path));
        }
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
            // The previous version's symbols describe code that no longer
            // exists: drop the file row (its symbols, relations, literals and
            // embeddings cascade) together with recording the issue, so the
            // index matches a fresh full index of the broken file — an issue,
            // no symbols — and a later fix or revert is re-parsed, not
            // hash-skipped against the stale row.
            let tx = index.conn.transaction()?;
            tx.execute(
                "DELETE FROM files WHERE relative_path = ?1",
                params![relative_path],
            )?;
            record_issue(&tx, &relative_path, UnsupportedKind::SyntaxError, &detail)?;
            tx.commit()?;
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
/// A path is judged as written, so on a case-insensitive filesystem the old
/// spelling of a case-only rename (`Foo.rs` → `foo.rs`) still resolves; its
/// rows are dropped because the file now indexes under its on-disk spelling.
///
/// When a path is newly indexed (created, or the new end of a rename), the
/// likely old ends of an unreported rename are checked and dropped if gone
/// (see [`remove_renamed_away`]). An edit of an already indexed file or a
/// deletion checks nothing else, so no update costs anything per indexed
/// file. Excluded paths and paths outside the root are ignored; the root
/// itself, or an ignore file whose rules changed ([`ExcludeSet::reload`]),
/// falls back to a full [`reindex`]. Relations are resolved at query time
/// (the `relation_candidates` view) and a symbol's embedding/literals
/// cascade with it, so rewriting one file's rows is all an update needs: the
/// result is the same index a full [`reindex`] would produce.
pub fn reindex_paths(
    index: &mut Index,
    registry: &LanguageRegistry,
    paths: &[PathBuf],
) -> Result<ReindexReport> {
    let mut report = ReindexReport::default();
    let root = index.root.clone();
    let exclude = index.exclude.clone();
    // Paths this batch indexed for the first time.
    let mut created = Vec::new();

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

    // An edited ignore file can exclude or un-exclude paths anywhere, none of
    // which were reported: only a full walk applies the new rules.
    let rules_file_changed = unique.iter().any(|p| {
        to_relative_slash_path(&root, p).is_some_and(|rel| ExcludeSet::is_rules_file(&rel))
    });
    if rules_file_changed && exclude.reload() {
        return reindex(index, registry, false);
    }

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

        // `symlink_metadata`, not `is_dir`: a symlink to a directory (maybe
        // outside the root) is never walked, as in a full reindex.
        let metadata = absolute.symlink_metadata();
        if metadata.as_ref().is_ok_and(|m| m.is_dir()) {
            let mut seen = Vec::new();
            let mut seen_manifests = Vec::new();
            let mut unreadable = Vec::new();
            for entry in WalkDir::new(&absolute).into_iter().filter_entry(|entry| {
                entry.depth() == 0 || walk_entry_allowed(&root, entry, &exclude)
            }) {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        unreadable.push(record_walk_failure(index, &err, &mut report)?);
                        continue;
                    }
                };
                if entry.file_type().is_dir() {
                    continue;
                }
                let as_walked = to_relative_slash_path(&root, entry.path()).unwrap_or_default();
                match index_noting_created(
                    index,
                    registry,
                    entry.path(),
                    &as_walked,
                    &mut report,
                    &mut created,
                )? {
                    Seen::Source(p) => seen.push(p),
                    Seen::Manifest(p) => seen_manifests.push(p),
                    Seen::Unreadable(p) => unreadable.push(p),
                    Seen::Nothing => {}
                }
            }
            report.files_removed +=
                remove_under_prefix(index, &relative_path, &seen, &seen_manifests, &unreadable)?;
        } else if metadata.is_ok() {
            let (indexed_as, manifest) = match index_noting_created(
                index,
                registry,
                &absolute,
                &relative_path,
                &mut report,
                &mut created,
            )? {
                Seen::Source(p) => (Some(p), false),
                Seen::Manifest(p) => (Some(p), true),
                // Its rows stay as they are, like any path indexed as reported.
                Seen::Unreadable(p) => (Some(p), false),
                Seen::Nothing => (None, false),
            };
            // Indexed under another spelling (the old name of a case-only
            // rename, a symlink resolving elsewhere) or not at all (now
            // excluded or escaping the root): rows under the reported
            // spelling are stale. A no-op for a plain edit.
            if indexed_as.as_deref() != Some(relative_path.as_str()) {
                let keep: Vec<String> = indexed_as.into_iter().collect();
                let (files, manifests) = if manifest {
                    (&[][..], &keep[..])
                } else {
                    (&keep[..], &[][..])
                };
                report.files_removed +=
                    remove_under_prefix(index, &relative_path, files, manifests, &[])?;
            }
        } else if metadata
            .as_ref()
            .is_err_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied)
        {
            // Can't even stat it (a parent lost its search permission): it
            // may still exist, so its rows stay, with the failure on record.
            let detail = metadata.err().map(|e| e.to_string()).unwrap_or_default();
            record_read_failure(index, &mut report, &relative_path, detail)?;
        } else {
            report.files_removed += remove_under_prefix(index, &relative_path, &[], &[], &[])?;
        }
    }

    if !created.is_empty() {
        report.files_removed += remove_renamed_away(index, &created)?;
    }
    touch_last_indexed_at(index)?;
    Ok(report)
}

/// Whether any `files`, `index_issues` or `dependencies` row exists for
/// exactly `relative_path`.
fn has_rows(index: &Index, relative_path: &str) -> Result<bool> {
    Ok(index.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM files WHERE relative_path = ?1)
             OR EXISTS(SELECT 1 FROM index_issues WHERE relative_path = ?1)
             OR EXISTS(SELECT 1 FROM dependencies WHERE manifest_path = ?1)",
        params![relative_path],
        |row| row.get(0),
    )?)
}

/// Whether `relative_path` (as stored: canonical, `/`-separated) still names
/// a file on disk under exactly that spelling. Canonicalizing, rather than a
/// `stat`, is what catches a case-only rename on a case-insensitive
/// filesystem (macOS, Windows): the old spelling still resolves, but to the
/// new one. It also rejects a path that now runs through a symlink, which a
/// full reindex would index under its target instead.
fn on_disk_as_stored(root: &Path, relative_path: &str) -> bool {
    let path = relative_path
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part));
    path.canonicalize().is_ok_and(|canonical| canonical == path)
}

/// [`index_file`], noting in `created` a path it indexed that had no rows
/// before under the spelling it was reported or walked as.
fn index_noting_created(
    index: &mut Index,
    registry: &LanguageRegistry,
    path: &Path,
    as_reported: &str,
    report: &mut ReindexReport,
    created: &mut Vec<String>,
) -> Result<Seen> {
    let was_indexed = has_rows(index, as_reported)?;
    let seen = index_file(index, registry, path, false, report)?;
    if let Seen::Source(p) | Seen::Manifest(p) = &seen {
        if !was_indexed && has_rows(index, p)? {
            created.push(p.clone());
        }
    }
    Ok(seen)
}

const PATH_TABLES: [(&str, &str); 3] = [
    ("files", "relative_path"),
    ("index_issues", "relative_path"),
    ("dependencies", "manifest_path"),
];

/// The old end of a rename that was reported by its new path only — which
/// is what macOS FSEvents delivers through the debouncer's file-id cache —
/// would otherwise stay indexed. For the `created` paths of a batch, checks
/// the likely old ends and drops every row of those no longer on disk
/// ([`on_disk_as_stored`]):
///
/// - indexed files with the same content hash as a created one (a rename,
///   within or across directories, of a file or a whole directory);
/// - indexed paths directly in a created path's directory (a rename that
///   also edited the file, a case-only rename).
///
/// When such a same-hash file vanished from a directory that is itself gone
/// ([`renamed_directory`]), everything still indexed under that directory
/// goes too — manifests and syntax errors, which have no hash to match.
///
/// Bounded by what the batch created, never by the size of the index: an
/// unreported move to another directory that also changed the content is
/// the one case left to the next full reindex, like any lost event.
/// Known gap: a directory renamed by its new path only with no same-hash candidate (only manifests/broken files) keeps its old rows until the next full reindex.
fn remove_renamed_away(index: &mut Index, created: &[String]) -> Result<usize> {
    let root = index.root.clone();
    let created_set: HashSet<&str> = created.iter().map(String::as_str).collect();
    let mut candidates: Vec<String> = Vec::new();

    // Created paths by content hash, and the indexed paths sharing one.
    let mut created_by_hash: HashMap<String, Vec<&str>> = HashMap::new();
    for path in created {
        let hash: Option<Option<String>> = index
            .conn
            .query_row(
                "SELECT content_hash FROM files WHERE relative_path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(hash) = hash.flatten() {
            created_by_hash.entry(hash).or_default().push(path);
        }
    }
    let hashes: Vec<&String> = created_by_hash.keys().collect();
    let mut same_hash: Vec<(String, String)> = Vec::new();
    for chunk in hashes.chunks(500) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let mut stmt = index.conn.prepare(&format!(
            "SELECT relative_path, content_hash FROM files WHERE content_hash IN ({placeholders})"
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk), |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        same_hash.extend(rows.collect::<rusqlite::Result<Vec<(String, String)>>>()?);
    }
    candidates.extend(same_hash.iter().map(|(path, _)| path.clone()));

    let parents: HashSet<&str> = created
        .iter()
        .map(|p| p.rsplit_once('/').map_or("", |(dir, _)| dir))
        .collect();
    for parent in parents {
        let prefix = format!("{parent}/");
        let prefix_end = format!("{parent}0");
        for (table, column) in PATH_TABLES {
            let at_root = parent.is_empty();
            let mut stmt = index
                .conn
                .prepare_cached(&children_sql(table, column, at_root))?;
            let bound: &[&dyn rusqlite::ToSql] = if at_root {
                &[]
            } else {
                &[&prefix, &prefix_end]
            };
            let rows = stmt.query_map(bound, |row| row.get(0))?;
            candidates.extend(rows.collect::<rusqlite::Result<Vec<String>>>()?);
        }
    }

    candidates.sort();
    candidates.dedup();
    let vanished: Vec<&String> = candidates
        .iter()
        .filter(|path| !created_set.contains(path.as_str()) && !on_disk_as_stored(&root, path))
        .collect();
    if vanished.is_empty() {
        return Ok(0);
    }

    // A vanished file with the content of a created one whose path ends the
    // same way was moved along with its directory: rows under that old
    // directory without a content hash (a manifest's dependencies, a file's
    // syntax error) are stale too once the directory is gone.
    let vanished_set: HashSet<&str> = vanished.iter().map(|p| p.as_str()).collect();
    let mut old_dirs: Vec<&str> = same_hash
        .iter()
        .filter(|(path, _)| vanished_set.contains(path.as_str()))
        .flat_map(|(path, hash)| {
            created_by_hash
                .get(hash)
                .into_iter()
                .flatten()
                .filter_map(move |new| renamed_directory(path, new))
        })
        .filter(|dir| !on_disk_as_stored(&root, dir))
        .collect();
    old_dirs.sort_unstable();
    old_dirs.dedup();
    let old_dirs: Vec<String> = old_dirs.into_iter().map(str::to_owned).collect();

    let tx = index.conn.transaction()?;
    let mut removed = 0;
    for path in vanished {
        for (table, column) in PATH_TABLES {
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
    for dir in &old_dirs {
        removed += remove_under_prefix(index, dir, &[], &[], &[])?;
    }
    Ok(removed)
}

/// The directory a rename moved `old` out of, when `old` and `new` end in
/// the same path components (`lib/a/x.rs` → `pkg/a/x.rs` gives `lib`).
/// `None` when not even the file name matches or `old` sat at the root.
fn renamed_directory<'a>(old: &'a str, new: &str) -> Option<&'a str> {
    let old_parts: Vec<&str> = old.split('/').collect();
    let shared = old_parts
        .iter()
        .rev()
        .zip(new.split('/').rev())
        .take_while(|(a, b)| **a == *b)
        .count();
    if shared == 0 || shared >= old_parts.len() {
        return None;
    }
    let dir = &old_parts[..old_parts.len() - shared];
    let len = dir.iter().map(|part| part.len()).sum::<usize>() + dir.len() - 1;
    old.get(..len)
}

/// `column` values of `table` directly in a directory: `?1` (`dir/`) ..
/// `?2` (`dir0`) with no further `/`, or with no `/` at all at the root.
fn children_sql(table: &str, column: &str, at_root: bool) -> String {
    if at_root {
        format!("SELECT DISTINCT {column} FROM {table} WHERE instr({column}, '/') = 0")
    } else {
        format!(
            "SELECT DISTINCT {column} FROM {table}
             WHERE {column} >= ?1 AND {column} < ?2
               AND instr(substr({column}, length(?1) + 1), '/') = 0"
        )
    }
}

/// Drops every `files`, `index_issues` and `dependencies` row for
/// `relative_path` itself or for anything under it as a directory, except
/// the paths in `keep`/`keep_manifests` and everything at or under an
/// `unreadable` path (see [`is_retained`]). Returns how many `files` rows went.
fn remove_under_prefix(
    index: &mut Index,
    relative_path: &str,
    keep: &[String],
    keep_manifests: &[String],
    unreadable: &[String],
) -> Result<usize> {
    let keep: HashSet<&str> = keep.iter().map(String::as_str).collect();
    let keep_manifests: HashSet<&str> = keep_manifests.iter().map(String::as_str).collect();
    // `dir/` ≤ path < `dir0` (`0` is the byte after `/`) selects exactly the
    // paths under `dir/` and, unlike `LIKE` (`_`/`%` wildcards) or `substr`,
    // is a range the column's index serves. Table/column names are static,
    // never input.
    let prefix = format!("{relative_path}/");
    let prefix_end = format!("{relative_path}0");
    let tx = index.conn.transaction()?;
    let mut removed = 0;
    for (table, column, keep) in [
        ("files", "relative_path", &keep),
        ("index_issues", "relative_path", &keep),
        ("dependencies", "manifest_path", &keep_manifests),
    ] {
        let stored: Vec<String> = {
            let mut stmt = tx.prepare(&under_prefix_sql(table, column))?;
            let rows =
                stmt.query_map(params![relative_path, prefix, prefix_end], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for path in stored
            .iter()
            .filter(|p| !keep.contains(p.as_str()) && !is_retained(p, unreadable))
        {
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

/// `column` values of `table` equal to `?1` or under the `?2` (`dir/`) ..
/// `?3` (`dir0`) range — see [`remove_under_prefix`].
fn under_prefix_sql(table: &str, column: &str) -> String {
    format!(
        "SELECT DISTINCT {column} FROM {table}
         WHERE {column} = ?1 OR ({column} >= ?2 AND {column} < ?3)"
    )
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

fn remove_missing_manifests(
    index: &mut Index,
    seen_manifest_paths: &[String],
    unreadable: &[String],
) -> Result<()> {
    let tx = index.conn.transaction()?;
    let seen: std::collections::HashSet<&str> =
        seen_manifest_paths.iter().map(String::as_str).collect();

    let mut stmt = tx.prepare("SELECT DISTINCT manifest_path FROM dependencies")?;
    let stored: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    for path in &stored {
        if !seen.contains(path.as_str()) && !is_retained(path, unreadable) {
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

    let targets: HashMap<usize, &mct_core::RelationTarget> = parsed
        .relation_targets
        .iter()
        .map(|t| (t.relation, t))
        .collect();
    for (i, relation) in parsed.relations.iter().enumerate() {
        let Some(&from_row_id) = id_map.get(&relation.from) else {
            continue; // parser bug guard: relation referencing an unknown local symbol id
        };
        let target = targets.get(&i);
        tx.execute(
            "INSERT INTO relations (from_symbol_id, kind, to_name, line, column, byte_len,
                                    qualifier, target_path, target_language, external, targets_parsed,
                                    target_kind, target_module, module, member)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11, ?12, ?13, ?14)",
            params![
                from_row_id,
                relation_kind_str(relation.kind),
                relation.to_name,
                relation.location.line,
                relation.location.column,
                relation.location.byte_len,
                target.and_then(|t| t.qualifier.as_deref()),
                target.and_then(|t| t.path.as_deref()),
                target.and_then(|t| t.language.as_deref()),
                target.is_some_and(|t| t.external),
                target.and_then(|t| t.kind).map(symbol_kind_str),
                target.and_then(|t| t.target_module.as_deref()),
                target.and_then(|t| t.module.as_deref()),
                target.is_some_and(|t| t.member),
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

fn remove_missing_files(
    index: &mut Index,
    seen_paths: &[String],
    unreadable: &[String],
) -> Result<usize> {
    let tx = index.conn.transaction()?;
    let seen: std::collections::HashSet<&str> = seen_paths.iter().map(String::as_str).collect();

    let mut stmt = tx.prepare("SELECT relative_path FROM files")?;
    let stored: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    let mut removed = 0;
    for path in &stored {
        if !seen.contains(path.as_str()) && !is_retained(path, unreadable) {
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
        if !seen.contains(path.as_str()) && !is_retained(&path, unreadable) {
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
        UnsupportedKind::ReadFailure => "read_failure",
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
    drop(stmt);

    let mut stmt = index.conn.prepare(
        "SELECT relative_path, detail FROM index_issues WHERE issue_kind = 'read_failure'
         ORDER BY relative_path",
    )?;
    let read_failures: Vec<UnsupportedFile> = stmt
        .query_map([], |row| {
            Ok(UnsupportedFile {
                relative_path: row.get(0)?,
                kind: UnsupportedKind::ReadFailure,
                detail: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

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
        read_failures,
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

#[cfg(test)]
mod prefix_query_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn the_renamed_directory_is_the_part_before_the_shared_suffix() {
        assert_eq!(renamed_directory("lib/x.rs", "pkg/x.rs"), Some("lib"));
        assert_eq!(
            renamed_directory("a/lib/m/x.rs", "a/pkg/m/x.rs"),
            Some("a/lib")
        );
        assert_eq!(renamed_directory("lib/x.rs", "x.rs"), Some("lib"));
        // Renamed within a directory, or moved out of the root: no directory.
        assert_eq!(renamed_directory("lib/x.rs", "lib/y.rs"), None);
        assert_eq!(renamed_directory("x.rs", "pkg/x.rs"), None);
        assert_eq!(renamed_directory("x.rs", "x.rs"), None);
    }

    /// Deleting a path must not scan whole tables: each table's prefix
    /// query has to be served by an index on its path column.
    #[test]
    fn the_prefix_query_is_an_index_search_on_every_table() {
        let root = std::env::temp_dir();
        let index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        for (table, column) in [
            ("files", "relative_path"),
            ("index_issues", "relative_path"),
            ("dependencies", "manifest_path"),
        ] {
            let sql = format!("EXPLAIN QUERY PLAN {}", under_prefix_sql(table, column));
            let mut stmt = index.conn.prepare(&sql).unwrap();
            let plan: Vec<String> = stmt
                .query_map(params!["a", "a/", "a0"], |row| row.get(3))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            assert!(
                plan.iter().all(|step| !step.starts_with("SCAN")),
                "{table}: {plan:?}"
            );
            assert!(
                plan.iter().any(|step| step.contains("INDEX")),
                "{table}: {plan:?}"
            );

            let sql = format!("EXPLAIN QUERY PLAN {}", children_sql(table, column, false));
            let mut stmt = index.conn.prepare(&sql).unwrap();
            let plan: Vec<String> = stmt
                .query_map(params!["a/", "a0"], |row| row.get(3))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            assert!(
                plan.iter().all(|step| !step.starts_with("SCAN")),
                "{table} children: {plan:?}"
            );
        }
    }
}
