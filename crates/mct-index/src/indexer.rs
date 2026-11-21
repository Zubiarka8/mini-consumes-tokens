use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use git2::{ObjectType, Oid};
use mct_core::{LanguageRegistry, ParseError, SourceFile};
use rusqlite::{params, OptionalExtension};
use walkdir::WalkDir;

use crate::assets::{self, AssetError};
use crate::incidents;
use crate::manifests;
use crate::symbol_changes;
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
    /// Symbols this run added / removed, netted out per file (a symbol
    /// rewritten unchanged counts as neither). Only filled while
    /// [`Index::track_symbol_changes`] is on; zero otherwise.
    pub symbols_added: usize,
    pub symbols_removed: usize,
    /// Up to 20 removed symbols as `kind name (path)`, sorted;
    /// `symbols_removed` is the exact count. Same condition as above.
    pub removed_symbols: Vec<String>,
    pub issues: Vec<UnsupportedFile>,
    /// Non-fatal problems after the index itself was updated, e.g. a failed
    /// [`Index::set_post_reindex_hook`] callback. Empty on a clean run.
    pub warnings: Vec<String>,
    /// Whether this run changed the index and published a new generation.
    /// A run that found nothing to change (a periodic reconciliation of an
    /// up-to-date project) commits nothing: no `index_meta` write, no
    /// generation bump, so other processes' generation-keyed caches and the
    /// dashboard stay valid.
    pub published: bool,
}

/// One row of the index's file table; see [`Index::indexed_files`].
#[derive(Debug, Clone)]
pub struct IndexedFile {
    pub relative_path: String,
    pub language: String,
    pub content_hash: String,
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

/// One config that may declare module aliases.
#[derive(Debug, Clone)]
pub struct AliasSource {
    pub manifest_path: String,
    /// Alias targets recorded, after `extends`.
    pub aliases: usize,
    /// Why the config could not be read; it then declares no alias.
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct IndexStatus {
    pub languages: Vec<LanguageCoverage>,
    /// Module-alias sources read (`tsconfig.json`/`jsconfig.json` `paths`,
    /// issue #563), including those that could not be parsed.
    pub alias_sources: Vec<AliasSource>,
    pub total_files: usize,
    pub total_symbols: usize,
    /// The last run that changed the index (published a generation).
    pub last_indexed_at: Option<i64>,
    /// The last run of this process that completed, whether or not it changed
    /// anything. Process-local: a run that changes nothing writes nothing to
    /// the database, so other processes do not see it.
    pub last_verified_at: Option<i64>,
    /// Monotonic published-change sequence. Legacy indexes without the key report 0.
    pub published_generation: u64,
    /// Most recent full walk that completed without an unreadable path: the
    /// later of the last published one and this process's last unchanged one.
    pub last_full_reconciled_at: Option<i64>,
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
    atomic_run(index, "reindex", |index| {
        reindex_inner(index, registry, force)
    })
}

fn reindex_inner(
    index: &mut Index,
    registry: &LanguageRegistry,
    force: bool,
) -> Result<ReindexReport> {
    let mut report = ReindexReport::default();
    let root = index.root.clone();
    symbol_changes::begin(&index.conn)?;
    let changes_before = index.conn.total_changes();

    let mut seen_paths = Vec::new();
    let mut seen_manifest_paths = Vec::new();
    // Paths that could not be read: the sweeps below must not take them (or
    // anything under them) for deleted.
    let mut unreadable: Vec<String> = Vec::new();

    // Pick up edits to the project's ignore files first: the walk below then
    // skips newly excluded paths (and the sweep drops their rows) and indexes
    // newly un-excluded ones, forced or not.
    index.exclude.try_reload_discovering()?;
    // Read before the walk: a reload by another clone mid-walk leaves this
    // run's rules behind the set's, and the next check sweeps again.
    let generation = index.exclude.generation();

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
    crate::join_resolution::resolve(&index.conn)?;
    // A change of rules publishes even when it moved no row: responses that
    // read the filesystem through the rules (the file tree) are cached by
    // generation too.
    let rules_changed = generation != index.applied_exclusions;
    report.published =
        publish_if_changed(index, changes_before, rules_changed, unreadable.is_empty())?;
    index.applied_exclusions = generation;
    symbol_changes::finish(&index.conn, &mut report)?;

    Ok(report)
}

/// Publishes a complete reindex run with one SQLite commit. A panic is caught
/// only long enough to roll back the transaction, then resumed unchanged.
/// Process termination also leaves SQLite to roll the open transaction back.
///
/// The incidents the run queued ([`queue_incident`]) are logged after the
/// commit; a rolled-back run drops them.
fn atomic_run<T>(
    index: &mut Index,
    name: &str,
    operation: impl FnOnce(&mut Index) -> Result<T>,
) -> Result<T> {
    index.conn.execute_batch("BEGIN IMMEDIATE")?;
    index.pending_incidents.borrow_mut().clear();
    match catch_unwind(AssertUnwindSafe(|| operation(index))) {
        Ok(Ok(value)) => match index.conn.execute_batch("COMMIT") {
            Ok(()) => {
                let pending = std::mem::take(&mut *index.pending_incidents.borrow_mut());
                incidents::flush_pending(&index.root, name, pending);
                Ok(value)
            }
            Err(error) => {
                let _ = index.conn.execute_batch("ROLLBACK");
                index.pending_incidents.borrow_mut().clear();
                Err(error.into())
            }
        },
        Ok(Err(error)) => {
            let rolled_back = index.conn.execute_batch("ROLLBACK");
            index.pending_incidents.borrow_mut().clear();
            match rolled_back {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(rollback_error.into()),
            }
        }
        Err(payload) => {
            let _ = index.conn.execute_batch("ROLLBACK");
            index.pending_incidents.borrow_mut().clear();
            resume_unwind(payload)
        }
    }
}

/// Queues an incident for the open run; see [`atomic_run`].
fn queue_incident(index: &Index, kind: &'static str, path: &str, line: Option<u32>, detail: &str) {
    index
        .pending_incidents
        .borrow_mut()
        .push(incidents::Pending {
            kind,
            path: path.to_string(),
            line,
            detail: detail.to_string(),
        });
}

/// Records a syntax error in `index_issues` and queues an incident only when
/// the stored issue is new or changed. A file that stays invalid with the same
/// diagnostic is re-parsed on every run (its `files` row is gone) but is not
/// logged again; a fix clears the row, so a later break logs afresh.
fn record_syntax_issue(
    index: &Index,
    relative_path: &str,
    detail: &str,
    line: Option<u32>,
) -> Result<()> {
    if record_issue(
        &index.conn,
        relative_path,
        UnsupportedKind::SyntaxError,
        detail,
    )? {
        queue_incident(index, "syntax-error", relative_path, line, detail);
    }
    Ok(())
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
    // One record per occurrence, unlike a syntax error.
    queue_incident(index, "read-failure", relative_path, None, &detail);
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
    if matches!(file_name, "CMakeLists.txt" | "compile_commands.json") {
        match crate::read_repository_file(&canonical) {
            Ok(bytes) => {
                clear_read_failure(index, &relative_path)?;
                let rows = if file_name == "CMakeLists.txt" {
                    cmake_include_rows(&relative_path, &String::from_utf8_lossy(&bytes))
                } else {
                    compile_commands_include_rows(index.root(), &bytes)
                };
                store_include_dirs(index, &relative_path, rows)?;
            }
            Err(err) => {
                record_read_failure(index, report, &relative_path, err.to_string())?;
                return Ok(Seen::Unreadable(relative_path));
            }
        }
        return Ok(Seen::Manifest(relative_path));
    }
    if matches!(file_name, "tsconfig.json" | "jsconfig.json") {
        match crate::read_repository_file(&canonical) {
            Ok(bytes) => {
                clear_read_failure(index, &relative_path)?;
                write_tsconfig(index, &relative_path, &bytes)?;
            }
            Err(err) => {
                record_read_failure(index, report, &relative_path, err.to_string())?;
                return Ok(Seen::Unreadable(relative_path));
            }
        }
        return Ok(Seen::Manifest(relative_path));
    }
    if let Some(language) = manifests::manifest_language(file_name) {
        match crate::read_repository_file(&canonical) {
            Ok(bytes) => {
                clear_read_failure(index, &relative_path)?;
                if manifests::is_rockspec(file_name) {
                    // Invalid UTF-8 is recorded, not skipped: skipping would
                    // leave the previous analysis standing as if current.
                    let analysis = match String::from_utf8(bytes) {
                        Ok(contents) => manifests::lua::analyse(&contents),
                        Err(_) => manifests::lua::Analysis::unreadable("not valid UTF-8"),
                    };
                    write_rockspec(index, &relative_path, language, &analysis)?;
                } else if let Ok(contents) = String::from_utf8(bytes) {
                    if file_name == "Cargo.toml" {
                        write_cargo_renames(index, &relative_path, &contents)?;
                        let name = manifests::cargo_library_name(&contents);
                        if let Some(name) = name {
                            let directory =
                                relative_path.rsplit_once('/').map_or("", |(dir, _)| dir);
                            index.conn.execute(
                                "INSERT INTO cargo_packages (manifest_path, directory, crate_name) VALUES (?1, ?2, ?3)
                                 ON CONFLICT(manifest_path) DO UPDATE SET directory = excluded.directory, crate_name = excluded.crate_name
                                 WHERE directory <> excluded.directory OR crate_name <> excluded.crate_name",
                                params![relative_path, directory, name],
                            )?;
                        } else {
                            index.conn.execute(
                                "DELETE FROM cargo_packages WHERE manifest_path = ?1",
                                params![relative_path],
                            )?;
                        }
                    }
                    if file_name == "package.json" {
                        write_package_imports(index, &relative_path, &contents)?;
                    }
                    if manifests::is_csproj(file_name) {
                        write_web_root(index, &relative_path, web_project_root(&contents))?;
                    }
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

    if let Some(extension) = assets::asset_extension(file_name) {
        let file_name = file_name.to_string();
        return index_asset(
            index,
            &canonical,
            relative_path,
            file_name,
            &extension,
            force,
            report,
        );
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
        Err(e) => {
            // Nothing from an earlier readable version may stay indexed.
            index.conn.execute(
                "DELETE FROM index_issues
                 WHERE relative_path = ?1 AND issue_kind = 'syntax_error'",
                params![relative_path],
            )?;
            if existing.is_some() {
                index.conn.execute(
                    "DELETE FROM files WHERE relative_path = ?1",
                    params![relative_path],
                )?;
            }
            // A NUL byte means a binary file that merely has a source
            // extension: nothing to parse, nothing to report. Any other
            // invalid UTF-8 is text in another encoding (Latin-1 PHP, a
            // Windows-1252 script): the file cannot be parsed, which the
            // status must say rather than silently omit its symbols.
            if e.as_bytes().contains(&0) {
                return Ok(Seen::Source(relative_path));
            }
            let offset = e.utf8_error().valid_up_to();
            let line = e.as_bytes()[..offset]
                .iter()
                .filter(|b| **b == b'\n')
                .count()
                + 1;
            let detail = format!(
                "line {line}: not valid UTF-8 (first invalid byte at offset {offset}); \
                 re-save the file as UTF-8 to index it"
            );
            record_syntax_issue(index, &relative_path, &detail, u32::try_from(line).ok())?;
            report.issues.push(UnsupportedFile {
                relative_path: relative_path.clone(),
                kind: UnsupportedKind::SyntaxError,
                detail,
            });
            return Ok(Seen::Source(relative_path));
        }
    };

    let source = SourceFile {
        relative_path: relative_path.clone(),
        contents,
    };

    let parsed = registry.parse(&source);
    // Parsing can be expensive and the working tree is mutable. Verify the
    // exact bytes again before writing this file into the run so an edit made
    // during either a successful or failed parse cannot be published as current.
    let current_hash = crate::read_repository_file(&canonical)
        .ok()
        .and_then(|bytes| Oid::hash_object(ObjectType::Blob, &bytes).ok())
        .map(|oid| oid.to_string());
    if current_hash.as_deref() != Some(content_hash.as_str()) {
        return Err(crate::IndexError::SourceChangedDuringIndex(relative_path));
    }

    match parsed {
        Ok(parsed) => {
            let language = parser.language_id();
            let symbols_written =
                write_parsed_file(index, &relative_path, language, &content_hash, &parsed)?;
            if crate::vite::is_vite_config(file_name) {
                write_vite_alias(index, &relative_path, &source.contents)?;
            }
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
            index.conn.execute(
                "DELETE FROM files WHERE relative_path = ?1",
                params![relative_path],
            )?;
            record_syntax_issue(index, &relative_path, &detail, Some(line))?;
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

/// Indexes one asset file (ADR-003 D1): a `files` row with language
/// `asset` and one `asset` symbol named `file_name`, hash-skipped on
/// `asset:<size>:<mtime-secs>`. Malformed, truncated or over-cap glTF JSON
/// keeps the symbol and records a `syntax_error` issue.
fn index_asset(
    index: &mut Index,
    canonical: &Path,
    relative_path: String,
    file_name: String,
    extension: &str,
    force: bool,
    report: &mut ReindexReport,
) -> Result<Seen> {
    let metadata = match std::fs::metadata(canonical) {
        Ok(m) => m,
        Err(err) => {
            record_read_failure(index, report, &relative_path, err.to_string())?;
            return Ok(Seen::Unreadable(relative_path));
        }
    };
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let content_hash = format!("asset:{}:{mtime}", metadata.len());
    let existing: Option<String> = index
        .conn
        .query_row(
            "SELECT content_hash FROM files WHERE relative_path = ?1",
            params![relative_path],
            |row| row.get(0),
        )
        .optional()?;
    if !force && existing.as_deref() == Some(content_hash.as_str()) {
        clear_read_failure(index, &relative_path)?;
        report.files_unchanged += 1;
        return Ok(Seen::Source(relative_path));
    }

    let issue = match assets::read_gltf_json(canonical, extension, metadata.len()) {
        Ok(_) => None,
        Err(AssetError::Invalid(detail)) => Some(detail),
        Err(AssetError::Io(err)) => {
            record_read_failure(index, report, &relative_path, err.to_string())?;
            return Ok(Seen::Unreadable(relative_path));
        }
    };
    let parsed = mct_core::ParsedFile {
        symbols: vec![mct_core::SymbolRecord {
            id: 0,
            name: file_name,
            kind: mct_core::SymbolKind::Asset,
            location: mct_core::Location {
                line: 1,
                column: 1,
                byte_len: 0,
                end_line: None,
            },
            parent: None,
            level: None,
        }],
        ..Default::default()
    };
    report.symbols_written += write_parsed_file(
        index,
        &relative_path,
        assets::ASSET_LANGUAGE,
        &content_hash,
        &parsed,
    )?;
    report.files_parsed += 1;
    if let Some(detail) = issue {
        record_syntax_issue(index, &relative_path, &detail, None)?;
        report.issues.push(UnsupportedFile {
            relative_path: relative_path.clone(),
            kind: UnsupportedKind::SyntaxError,
            detail,
        });
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
    atomic_run(index, "reindex", |index| {
        reindex_paths_inner(index, registry, paths)
    })
}

fn reindex_paths_inner(
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
    let rules_file_changed = unique
        .iter()
        .any(|p| to_relative_slash_path(&root, p).is_some_and(|rel| exclude.is_rules_path(&rel)));
    if rules_file_changed && exclude.reload() {
        return reindex_inner(index, registry, false);
    }

    symbol_changes::begin(&index.conn)?;
    let changes_before = index.conn.total_changes();

    for absolute in unique {
        // Judged as written, not canonicalized: a deleted path can't be.
        let Some(relative_path) = to_relative_slash_path(&root, &absolute) else {
            continue;
        };
        if relative_path.is_empty() {
            return reindex_inner(index, registry, false);
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
    crate::join_resolution::resolve(&index.conn)?;
    report.published = publish_if_changed(index, changes_before, false, false)?;
    symbol_changes::finish(&index.conn, &mut report)?;
    Ok(report)
}

/// Whether any `files`, `index_issues`, `dependencies` or `rockspec_items` row exists for
/// exactly `relative_path`.
fn has_rows(index: &Index, relative_path: &str) -> Result<bool> {
    Ok(index.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM files WHERE relative_path = ?1)
             OR EXISTS(SELECT 1 FROM index_issues WHERE relative_path = ?1)
             OR EXISTS(SELECT 1 FROM dependencies WHERE manifest_path = ?1)
             OR EXISTS(SELECT 1 FROM rockspec_items WHERE manifest_path = ?1)
             OR EXISTS(SELECT 1 FROM cargo_packages WHERE manifest_path = ?1)
             OR EXISTS(SELECT 1 FROM ts_configs WHERE manifest_path = ?1)",
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

const PATH_TABLES: [(&str, &str); 6] = [
    ("files", "relative_path"),
    ("index_issues", "relative_path"),
    ("dependencies", "manifest_path"),
    ("rockspec_items", "manifest_path"),
    ("cargo_packages", "manifest_path"),
    ("ts_configs", "manifest_path"),
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

    let tx = &index.conn;
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
            if table == "ts_configs" && deleted > 0 {
                refresh_extending_tsconfigs(index, path)?;
            }
        }
    }
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

/// Drops every `files`, `index_issues`, `dependencies` and `rockspec_items` row for
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
    let tx = &index.conn;
    let mut removed = 0;
    for (table, column, keep) in [
        ("files", "relative_path", &keep),
        ("index_issues", "relative_path", &keep),
        ("dependencies", "manifest_path", &keep_manifests),
        ("rockspec_items", "manifest_path", &keep_manifests),
        ("cargo_packages", "manifest_path", &keep_manifests),
        ("ts_configs", "manifest_path", &keep_manifests),
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
            if table == "ts_configs" && deleted > 0 {
                refresh_extending_tsconfigs(index, path)?;
            }
        }
    }
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
    let tx = &index.conn;
    // Manifests are re-read on every walk; rewriting identical rows would
    // make every reconciliation look like a change (see `publish_if_changed`).
    let stored: Vec<(String, String, Option<String>)> = tx
        .prepare_cached(
            "SELECT language, name, version FROM dependencies
             WHERE manifest_path = ?1 ORDER BY id",
        )?
        .query_map(params![manifest_path], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let unchanged = stored.len() == deps.len()
        && stored.iter().zip(deps).all(|((lang, name, version), dep)| {
            lang == language && *name == dep.name && *version == dep.version
        });
    if unchanged {
        return Ok(());
    }
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
    Ok(())
}

/// Writes a rockspec's analysis: the detail rows and, from the same analysis
/// in the same transaction, the name/version summary other consumers read.
/// Both are replaced together so they cannot disagree.
fn write_rockspec(
    index: &mut Index,
    manifest_path: &str,
    language: &str,
    analysis: &manifests::lua::Analysis,
) -> Result<()> {
    let summary = manifests::dedupe_by_name(analysis.projection());
    write_manifest_dependencies(index, manifest_path, language, &summary)?;
    let tx = &index.conn;
    if rockspec_items_unchanged(tx, manifest_path, &analysis.items)? {
        return Ok(());
    }
    tx.execute(
        "DELETE FROM rockspec_items WHERE manifest_path = ?1",
        params![manifest_path],
    )?;
    let mut insert = tx.prepare_cached(
        "INSERT INTO rockspec_items
            (manifest_path, position, kind, section, scope, platform, namespace, name,
             version_constraint, path, native, source, state, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )?;
    for (position, item) in analysis.items.iter().enumerate() {
        insert.execute(params![
            manifest_path,
            position as i64,
            item.kind,
            item.section,
            item.scope,
            item.platform,
            item.namespace,
            item.name,
            item.constraint,
            item.path,
            item.native,
            item.source,
            item.state,
            item.detail,
        ])?;
    }
    Ok(())
}

/// Whether the stored detail rows of `manifest_path` are exactly `items`, in
/// order, so rewriting them would change nothing.
fn rockspec_items_unchanged(
    conn: &rusqlite::Connection,
    manifest_path: &str,
    items: &[manifests::lua::Item],
) -> Result<bool> {
    use rusqlite::types::Value;
    fn text(value: Option<&str>) -> Value {
        value.map_or(Value::Null, |v| Value::Text(v.to_string()))
    }
    let mut stmt = conn.prepare_cached(
        "SELECT position, kind, section, scope, platform, namespace, name,
                version_constraint, path, native, source, state, detail
         FROM rockspec_items WHERE manifest_path = ?1 ORDER BY position",
    )?;
    let stored: Vec<Vec<Value>> = stmt
        .query_map(params![manifest_path], |row| {
            (0..13).map(|i| row.get::<_, Value>(i)).collect()
        })?
        .collect::<rusqlite::Result<_>>()?;
    if stored.len() != items.len() {
        return Ok(false);
    }
    Ok(stored
        .iter()
        .zip(items)
        .enumerate()
        .all(|(position, (row, item))| {
            let expected = [
                Value::Integer(position as i64),
                Value::Text(item.kind.to_string()),
                text(item.section),
                text(item.scope),
                text(item.platform.as_deref()),
                text(item.namespace.as_deref()),
                Value::Text(item.name.clone()),
                text(item.constraint.as_deref()),
                text(item.path.as_deref()),
                Value::Integer(i64::from(item.native)),
                text(item.source),
                Value::Text(item.state.to_string()),
                text(item.detail.as_deref()),
            ];
            row.as_slice() == expected.as_slice()
        }))
}

fn remove_missing_manifests(
    index: &mut Index,
    seen_manifest_paths: &[String],
    unreadable: &[String],
) -> Result<()> {
    let tx = &index.conn;
    let seen: std::collections::HashSet<&str> =
        seen_manifest_paths.iter().map(String::as_str).collect();

    // The union: a rockspec that declares nothing has no `dependencies` row
    // but still has detail rows, and must be swept like any other manifest.
    let mut stmt = tx.prepare(
        "SELECT manifest_path FROM dependencies
         UNION SELECT manifest_path FROM rockspec_items
         UNION SELECT manifest_path FROM cargo_packages
         UNION SELECT manifest_path FROM ts_configs
         UNION SELECT manifest_path FROM web_roots
         UNION SELECT manifest_path FROM package_manifests
         UNION SELECT manifest_path FROM cargo_renames
         UNION SELECT manifest_path FROM include_dirs",
    )?;
    let stored: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    for path in &stored {
        if index.exclude.is_excluded(path)
            || (!seen.contains(path.as_str()) && !is_retained(path, unreadable))
        {
            tx.execute(
                "DELETE FROM dependencies WHERE manifest_path = ?1",
                params![path],
            )?;
            tx.execute(
                "DELETE FROM rockspec_items WHERE manifest_path = ?1",
                params![path],
            )?;
            tx.execute(
                "DELETE FROM cargo_packages WHERE manifest_path = ?1",
                params![path],
            )?;
            tx.execute(
                "DELETE FROM ts_configs WHERE manifest_path = ?1",
                params![path],
            )?;
            tx.execute(
                "DELETE FROM web_roots WHERE manifest_path = ?1",
                params![path],
            )?;
            tx.execute(
                "DELETE FROM package_manifests WHERE manifest_path = ?1",
                params![path],
            )?;
            tx.execute(
                "DELETE FROM package_imports WHERE manifest_path = ?1",
                params![path],
            )?;
            tx.execute(
                "DELETE FROM cargo_renames WHERE manifest_path = ?1",
                params![path],
            )?;
            tx.execute(
                "DELETE FROM include_dirs WHERE manifest_path = ?1",
                params![path],
            )?;
        }
    }

    Ok(())
}

/// The include directories a `CMakeLists.txt` declares, as `(scope,
/// include)` repository paths: each applies to files under the
/// CMakeLists' directory (issue #563, ADR-047).
fn cmake_include_rows(path: &str, contents: &str) -> Vec<(String, String)> {
    let directory = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    cmake_include_dirs(contents)
        .into_iter()
        .filter_map(|dir| {
            let spec = format!("./{}", dir.trim_end_matches('/'));
            let resolved = mct_core::resolve_reference_path(path, &spec)?;
            // `./.` names the CMakeLists' own directory.
            let include = resolved.strip_suffix("/.").unwrap_or(&resolved);
            Some((directory.to_string(), include.to_string()))
        })
        .collect()
}

/// The `-I`/`-iquote` directories of each `compile_commands.json` entry, as
/// `(scope, include)` repository paths: each applies to files in the
/// compiled file's directory. Relative paths are taken from the entry's
/// `directory`; a path outside the repository, or an unparsable file,
/// gives nothing (issue #563, ADR-047).
fn compile_commands_include_rows(root: &Path, bytes: &[u8]) -> Vec<(String, String)> {
    let Ok(serde_json::Value::Array(entries)) = serde_json::from_slice(bytes) else {
        return Vec::new();
    };
    // Build tools write plain paths; the root may be canonical, which on
    // Windows carries the `\\?\` verbatim prefix.
    let roots: Vec<PathBuf> = std::iter::once(root.to_path_buf())
        .chain(root.canonicalize().ok())
        .flat_map(|r| {
            let plain = r
                .to_str()
                .and_then(|s| s.strip_prefix(r"\\?\"))
                .map(PathBuf::from);
            std::iter::once(r).chain(plain)
        })
        .collect();
    let inside = |base: &Path, path: &str| -> Option<String> {
        let mut normal = PathBuf::new();
        for part in base.join(path).components() {
            match part {
                std::path::Component::ParentDir => {
                    normal.pop();
                }
                std::path::Component::CurDir => {}
                other => normal.push(other),
            }
        }
        let relative = roots.iter().find_map(|r| normal.strip_prefix(r).ok())?;
        Some(relative.to_string_lossy().replace('\\', "/"))
    };
    let mut rows = Vec::new();
    for entry in &entries {
        let field = |name: &str| entry.get(name).and_then(serde_json::Value::as_str);
        let (Some(directory), Some(file)) = (field("directory"), field("file")) else {
            continue;
        };
        let base = Path::new(directory);
        let Some(file) = inside(base, file) else {
            continue;
        };
        let scope = file.rsplit_once('/').map_or("", |(dir, _)| dir).to_string();
        let words: Vec<&str> = match (entry.get("arguments"), field("command")) {
            (Some(serde_json::Value::Array(arguments)), _) => arguments
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect(),
            (_, Some(command)) => command.split_whitespace().collect(),
            _ => continue,
        };
        let mut words = words.iter();
        while let Some(&word) = words.next() {
            let dir = match word {
                "-I" | "-iquote" => words.next().copied(),
                w => w.strip_prefix("-I").filter(|d| !d.is_empty()),
            };
            if let Some(include) = dir.and_then(|d| inside(base, d.trim_matches('"'))) {
                rows.push((scope.clone(), include));
            }
        }
    }
    rows
}

/// Stores `rows` as `path`'s include directories, first occurrence of each
/// kept, leaving the table untouched when nothing changed.
fn store_include_dirs(index: &Index, path: &str, rows: Vec<(String, String)>) -> Result<()> {
    let rows = rows.into_iter().fold(Vec::new(), |mut rows, row| {
        if !rows.contains(&row) {
            rows.push(row);
        }
        rows
    });
    let stored: Vec<(String, String)> = {
        let mut stmt = index.conn.prepare(
            "SELECT directory, include_path FROM include_dirs WHERE manifest_path = ?1 ORDER BY position",
        )?;
        let found = stmt.query_map(params![path], |row| Ok((row.get(0)?, row.get(1)?)))?;
        found.collect::<rusqlite::Result<_>>()?
    };
    if stored == rows {
        return Ok(());
    }
    index.conn.execute(
        "DELETE FROM include_dirs WHERE manifest_path = ?1",
        params![path],
    )?;
    for (position, (directory, include_path)) in rows.iter().enumerate() {
        index.conn.execute(
            "INSERT INTO include_dirs (manifest_path, directory, position, include_path) VALUES (?1, ?2, ?3, ?4)",
            params![path, directory, position as i64, include_path],
        )?;
    }
    Ok(())
}

/// The literal directories of `include_directories(…)` and
/// `target_include_directories(<target> …)` calls, relative to the
/// CMakeLists' directory. Kept: plain relative paths,
/// `${CMAKE_CURRENT_SOURCE_DIR}/x` and `$<BUILD_INTERFACE:…>` around
/// either. Dropped: keywords, absolute paths, `$<INSTALL_INTERFACE:…>` and
/// anything holding another variable or generator expression.
fn cmake_include_dirs(contents: &str) -> Vec<String> {
    const KEYWORDS: [&str; 6] = [
        "PUBLIC",
        "PRIVATE",
        "INTERFACE",
        "SYSTEM",
        "BEFORE",
        "AFTER",
    ];
    let text: String = contents
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    for (command, skip_target) in [
        ("target_include_directories", true),
        ("include_directories", false),
    ] {
        let mut from = 0;
        while let Some(at) = lower[from..].find(command) {
            let start = from + at;
            from = start + command.len();
            let before = lower[..start].chars().next_back();
            if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                continue;
            }
            let rest = lower[from..].trim_start();
            if !rest.starts_with('(') {
                continue;
            }
            let open = text.len() - rest.len();
            let Some(close) = text[open..].find(')') else {
                break;
            };
            // A `)` inside `$<…>` or `${…}` ends nothing: find the call's own.
            let mut depth = 0i32;
            let mut end = None;
            for (i, c) in text[open..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(open + i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let end = end.unwrap_or(open + close);
            let args = &text[open + 1..end];
            for (i, arg) in args.split_whitespace().enumerate() {
                if (skip_target && i == 0) || KEYWORDS.contains(&arg) {
                    continue;
                }
                let arg = arg.trim_matches('"');
                let arg = arg
                    .strip_prefix("$<BUILD_INTERFACE:")
                    .and_then(|a| a.strip_suffix('>'))
                    .unwrap_or(arg);
                let arg = arg
                    .strip_prefix("${CMAKE_CURRENT_SOURCE_DIR}")
                    .map(|a| a.trim_start_matches('/'))
                    .map(|a| if a.is_empty() { "." } else { a })
                    .unwrap_or(arg);
                if arg.is_empty() || arg.starts_with('/') || arg.contains('$') || arg.contains(':')
                {
                    continue;
                }
                if !out.iter().any(|d: &String| d == arg) {
                    out.push(arg.to_string());
                }
            }
            from = end;
        }
    }
    out
}

/// Records the renamed path dependencies a `Cargo.toml` declares, each
/// resolved to the repository directory it names; a path leaving the
/// repository is dropped (issue #563, ADR-046).
fn write_cargo_renames(index: &Index, path: &str, contents: &str) -> Result<()> {
    let directory = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let rows: Vec<(String, String)> = manifests::cargo_path_renames(contents)
        .into_iter()
        .filter_map(|(alias, target)| {
            let spec = if target.starts_with("../") || target.starts_with("./") {
                target
            } else {
                format!("./{target}")
            };
            let resolved = mct_core::resolve_reference_path(path, spec.trim_end_matches('/'))?;
            Some((alias, resolved))
        })
        .collect();
    let stored: Vec<(String, String)> = {
        let mut stmt = index.conn.prepare(
            "SELECT alias, target_directory FROM cargo_renames WHERE manifest_path = ?1 ORDER BY rowid",
        )?;
        let found = stmt.query_map(params![path], |row| Ok((row.get(0)?, row.get(1)?)))?;
        found.collect::<rusqlite::Result<_>>()?
    };
    if stored == rows {
        return Ok(());
    }
    index.conn.execute(
        "DELETE FROM cargo_renames WHERE manifest_path = ?1",
        params![path],
    )?;
    for (alias, target) in &rows {
        index.conn.execute(
            "INSERT OR IGNORE INTO cargo_renames (manifest_path, directory, alias, target_directory)
             VALUES (?1, ?2, ?3, ?4)",
            params![path, directory, alias, target],
        )?;
    }
    Ok(())
}

/// Records a `package.json` as the package scope of the files under its
/// directory, with its literal `imports` map resolved to repository paths
/// (issue #563, ADR-045). An unparsable file keeps the scope with no map, so
/// an outer package never resolves files it does not govern.
fn write_package_imports(index: &Index, path: &str, contents: &str) -> Result<()> {
    let directory = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mappings = package_import_mappings(directory, contents);
    let name = serde_json::from_str::<serde_json::Value>(contents)
        .ok()
        .and_then(|doc| doc.get("name")?.as_str().map(str::to_string))
        .filter(|name| !name.is_empty());
    index.conn.execute(
        "INSERT INTO package_manifests (manifest_path, directory, name) VALUES (?1, ?2, ?3)
         ON CONFLICT(manifest_path) DO UPDATE SET directory = excluded.directory, name = excluded.name
         WHERE directory <> excluded.directory OR name IS NOT excluded.name",
        params![path, directory, name],
    )?;
    let stored: Vec<(String, String)> = {
        let mut stmt = index.conn.prepare(
            "SELECT pattern, target FROM package_imports WHERE manifest_path = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map(params![path], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    if stored == mappings {
        return Ok(());
    }
    index.conn.execute(
        "DELETE FROM package_imports WHERE manifest_path = ?1",
        params![path],
    )?;
    for (position, (pattern, target)) in mappings.iter().enumerate() {
        index.conn.execute(
            "INSERT INTO package_imports (manifest_path, position, pattern, target) VALUES (?1, ?2, ?3, ?4)",
            params![path, position as i64, pattern, target],
        )?;
    }
    Ok(())
}

/// `(pattern, repository target)` pairs of a package.json `imports` map.
/// Order carries no meaning: every target that exists stays a candidate,
/// as with tsconfig `paths` fallbacks. A key must start with `#` and hold at most one `*`, and each
/// target needs as many. String targets, condition objects (every string
/// leaf, in order) and arrays count. A target must be `./`-relative and stay
/// inside the package. A `.js`/`.mjs`/`.cjs` target is also kept without
/// its extension, so the TypeScript source it is emitted from matches.
fn package_import_mappings(directory: &str, contents: &str) -> Vec<(String, String)> {
    fn leaves<'a>(value: &'a serde_json::Value, out: &mut Vec<&'a str>) {
        match value {
            serde_json::Value::String(s) => out.push(s),
            serde_json::Value::Array(items) => items.iter().for_each(|v| leaves(v, out)),
            serde_json::Value::Object(map) => map.values().for_each(|v| leaves(v, out)),
            _ => {}
        }
    }
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(contents) else {
        return Vec::new();
    };
    let Some(imports) = doc.get("imports").and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (pattern, value) in imports {
        let stars = pattern.matches('*').count();
        if !pattern.starts_with('#') || pattern.len() < 2 || stars > 1 {
            continue;
        }
        let mut targets = Vec::new();
        leaves(value, &mut targets);
        for target in targets {
            let Some(rest) = target.strip_prefix("./") else {
                continue;
            };
            if target.matches('*').count() != stars
                || rest.split('/').any(|seg| seg == ".." || seg == ".")
            {
                continue;
            }
            let full = if directory.is_empty() {
                rest.to_string()
            } else {
                format!("{directory}/{rest}")
            };
            let stripped = [".js", ".mjs", ".cjs"]
                .iter()
                .find_map(|ext| full.strip_suffix(ext))
                .map(str::to_string);
            for candidate in std::iter::once(full).chain(stripped) {
                let pair = (pattern.clone(), candidate);
                if !out.contains(&pair) {
                    out.push(pair);
                }
            }
        }
    }
    out
}

/// The web root an ASP.NET project serves `/…` URLs from: `wwwroot` for a
/// web, Razor or Blazor WebAssembly SDK project, else none (issue #567).
fn web_project_root(csproj: &str) -> Option<&'static str> {
    [
        "Microsoft.NET.Sdk.Web",
        "Microsoft.NET.Sdk.Razor",
        "Microsoft.NET.Sdk.BlazorWebAssembly",
    ]
    .iter()
    .any(|sdk| csproj.contains(&format!("\"{sdk}\"")))
    .then_some("wwwroot")
}

/// Records (or clears) the web root a project file at `path` declares for
/// its directory; `relation_candidates` reads it for root-relative URLs.
fn write_web_root(index: &Index, path: &str, root: Option<&str>) -> Result<()> {
    match root {
        Some(root) => {
            let directory = path.rsplit_once('/').map_or("", |(dir, _)| dir);
            index.conn.execute(
                "INSERT INTO web_roots (manifest_path, directory, root) VALUES (?1, ?2, ?3)
                 ON CONFLICT(manifest_path) DO UPDATE SET directory = excluded.directory, root = excluded.root
                 WHERE directory <> excluded.directory OR root <> excluded.root",
                params![path, directory, root],
            )?;
        }
        None => {
            index.conn.execute(
                "DELETE FROM web_roots WHERE manifest_path = ?1",
                params![path],
            )?;
        }
    }
    Ok(())
}

/// Records the `@` alias of the Vite config at `path` (just written as a
/// source file) for files under its directory, or clears it (issue #563,
/// ADR-050). Its rows go with the file's `files` row.
fn write_vite_alias(index: &Index, path: &str, contents: &str) -> Result<()> {
    let directory = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let file_id: i64 = index.conn.query_row(
        "SELECT id FROM files WHERE relative_path = ?1",
        params![path],
        |row| row.get(0),
    )?;
    index.conn.execute(
        "DELETE FROM vite_aliases WHERE file_id = ?1",
        params![file_id],
    )?;
    let target = crate::vite::at_alias(contents).and_then(|dir| {
        if dir.is_empty() {
            return Some(directory.to_string());
        }
        mct_core::resolve_reference_path(path, &format!("./{dir}"))
    });
    if let Some(target) = target {
        let target = if target.is_empty() {
            "*".to_string()
        } else {
            format!("{target}/*")
        };
        index.conn.execute(
            "INSERT INTO vite_aliases (file_id, directory, pattern, target) VALUES (?1, ?2, '@/*', ?3)",
            params![file_id, directory, target],
        )?;
    }
    Ok(())
}

fn write_tsconfig(index: &Index, path: &str, bytes: &[u8]) -> Result<()> {
    let directory = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let doc = match serde_json::from_slice::<serde_json::Value>(&jsonc_to_json(bytes)) {
        Ok(doc) => doc,
        Err(err) => {
            // Kept as the nearest config with no aliases, so an unreadable
            // config never lets an outer one resolve its files, and listed
            // by `get_indexing_status`.
            index.conn.execute(
                "DELETE FROM ts_configs WHERE manifest_path = ?1",
                params![path],
            )?;
            index.conn.execute(
                "INSERT INTO ts_configs (manifest_path, directory, error) VALUES (?1, ?2, ?3)",
                params![path, directory, err.to_string()],
            )?;
            return refresh_extending_tsconfigs(index, path);
        }
    };
    let mut seen = vec![path.to_string()];
    let options = tsconfig_options(index.root(), path, &doc, &mut seen);
    let paths = options.mapped_paths();
    let parents: Vec<String> = seen.split_off(1);
    let old: Vec<(String, String)> = index
        .conn
        .prepare("SELECT pattern, target FROM ts_paths WHERE manifest_path = ?1 ORDER BY position")?
        .query_map(params![path], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let exists: bool = index.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ts_configs WHERE manifest_path = ?1 AND error IS NULL)",
        params![path],
        |row| row.get(0),
    )?;
    let old_parents: Vec<String> = index
        .conn
        .prepare("SELECT parent_path FROM ts_config_parents WHERE manifest_path = ?1 ORDER BY parent_path")?
        .query_map(params![path], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut sorted_parents = parents.clone();
    sorted_parents.sort();
    if exists && old == paths && old_parents == sorted_parents {
        return Ok(());
    }
    index.conn.execute(
        "DELETE FROM ts_configs WHERE manifest_path = ?1",
        params![path],
    )?;
    index.conn.execute(
        "INSERT INTO ts_configs (manifest_path, directory) VALUES (?1, ?2)",
        params![path, directory],
    )?;
    for (position, (pattern, target)) in paths.iter().enumerate() {
        index.conn.execute("INSERT INTO ts_paths (manifest_path, position, pattern, target) VALUES (?1, ?2, ?3, ?4)", params![path, position as i64, pattern, target])?;
    }
    for parent in &sorted_parents {
        index.conn.execute(
            "INSERT OR IGNORE INTO ts_config_parents (manifest_path, parent_path) VALUES (?1, ?2)",
            params![path, parent],
        )?;
    }
    refresh_extending_tsconfigs(index, path)
}

/// Re-reads every indexed config that `extends` `path`, so a change to a
/// base config reaches the configs inheriting its `paths` even when only
/// the base file changed. A config whose result is unchanged stops the
/// walk, which also ends an `extends` cycle.
fn refresh_extending_tsconfigs(index: &Index, path: &str) -> Result<()> {
    let children: Vec<String> = index
        .conn
        .prepare("SELECT manifest_path FROM ts_config_parents WHERE parent_path = ?1 AND manifest_path <> ?1")?
        .query_map(params![path], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for child in children {
        match crate::read_repository_file(&index.root().join(&child)) {
            Ok(bytes) => write_tsconfig(index, &child, &bytes)?,
            Err(_) => {
                index.conn.execute(
                    "DELETE FROM ts_configs WHERE manifest_path = ?1",
                    params![child],
                )?;
            }
        }
    }
    Ok(())
}

fn parse_tsconfig(bytes: &[u8]) -> Option<serde_json::Value> {
    serde_json::from_slice(&jsonc_to_json(bytes)).ok()
}

/// The longest `extends` chain followed; TypeScript itself has no limit, but
/// a deeper chain in one repository is a cycle or generated config.
const TSCONFIG_EXTENDS_DEPTH: usize = 8;

/// `baseUrl` and `paths` after `extends`, each with the directory that
/// resolves it: TypeScript resolves both relative to the config that
/// declares them, and a config's own value replaces an inherited one.
#[derive(Default)]
struct TsPathOptions {
    /// The `baseUrl` directory, repository-relative.
    base: Option<String>,
    /// The `paths` object and the directory of the config declaring it.
    paths: Option<(serde_json::Map<String, serde_json::Value>, String)>,
}

impl TsPathOptions {
    /// `(pattern, repository-relative target)` pairs in declaration order.
    /// Only patterns and targets with exactly one `*` and no absolute
    /// target are kept; a target climbing above the repository is dropped.
    fn mapped_paths(&self) -> Vec<(String, String)> {
        let Some((mapping, owner)) = &self.paths else {
            return Vec::new();
        };
        let base = self.base.as_deref().unwrap_or(owner);
        let mut out = Vec::new();
        for (pattern, targets) in mapping {
            if pattern.matches('*').count() != 1 {
                continue;
            }
            let Some(targets) = targets.as_array() else {
                continue;
            };
            for target in targets.iter().filter_map(serde_json::Value::as_str) {
                if target.matches('*').count() != 1 || target.starts_with('/') {
                    continue;
                }
                if let Some(target) =
                    mct_core::resolve_reference_path(&directory_file(base), &format!("./{target}"))
                {
                    out.push((pattern.clone(), target));
                }
            }
        }
        out
    }
}

/// A file name inside `directory`, for resolving `./x` against it.
fn directory_file(directory: &str) -> String {
    if directory.is_empty() {
        "tsconfig.json".to_string()
    } else {
        format!("{directory}/tsconfig.json")
    }
}

/// Reads `baseUrl` and `paths` from `doc` (the config at `path`) and, for
/// what it does not declare, from its relative `extends` configs in order.
/// A package `extends` (`@tsconfig/node18`) is skipped: it lives outside
/// the repository. `seen` breaks cycles.
fn tsconfig_options(
    root: &Path,
    path: &str,
    doc: &serde_json::Value,
    seen: &mut Vec<String>,
) -> TsPathOptions {
    let directory = path.rsplit_once('/').map_or("", |(dir, _)| dir).to_string();
    let compiler = &doc["compilerOptions"];
    let mut options = TsPathOptions {
        base: compiler["baseUrl"].as_str().and_then(|base| {
            if base.starts_with('/') {
                return None;
            }
            mct_core::resolve_reference_path(&directory_file(&directory), &format!("./{base}/x"))
                .map(|file| {
                    file.rsplit_once('/')
                        .map_or(String::new(), |(dir, _)| dir.to_string())
                })
        }),
        paths: compiler["paths"]
            .as_object()
            .map(|mapping| (mapping.clone(), directory.clone())),
    };
    if seen.len() > TSCONFIG_EXTENDS_DEPTH {
        return options;
    }
    let parents: Vec<&str> = match &doc["extends"] {
        serde_json::Value::String(one) => vec![one.as_str()],
        serde_json::Value::Array(many) => {
            many.iter().filter_map(serde_json::Value::as_str).collect()
        }
        _ => Vec::new(),
    };
    // With several parents, a later one overrides an earlier one, so the
    // last parent that declares a value wins.
    for parent in parents.into_iter().rev() {
        if options.base.is_some() && options.paths.is_some() {
            break;
        }
        if !(parent.starts_with("./") || parent.starts_with("../")) {
            continue;
        }
        let spec = if parent.ends_with(".json") {
            parent.to_string()
        } else {
            format!("{parent}.json")
        };
        let Some(parent_path) = mct_core::resolve_reference_path(path, &spec) else {
            continue;
        };
        if seen.contains(&parent_path) {
            continue;
        }
        let Some(parent_doc) = crate::read_repository_file(&root.join(&parent_path))
            .ok()
            .and_then(|bytes| parse_tsconfig(&bytes))
        else {
            continue;
        };
        seen.push(parent_path.clone());
        let inherited = tsconfig_options(root, &parent_path, &parent_doc, seen);
        options.base = options.base.or(inherited.base);
        options.paths = options.paths.or(inherited.paths);
    }
    options
}

/// TypeScript config files allow comments and trailing commas, but strings
/// must remain byte-for-byte intact (including escaped quotes and URLs).
fn jsonc_to_json(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let mut i = 0;
    let mut in_string = false;
    while i < out.len() {
        if in_string {
            if out[i] == b'\\' {
                i += 2;
                continue;
            }
            if out[i] == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if out[i] == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if out[i] == b'/' && out.get(i + 1) == Some(&b'/') {
            while i < out.len() && out[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
            continue;
        }
        if out[i] == b'/' && out.get(i + 1) == Some(&b'*') {
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            while i + 1 < out.len() && !(out[i] == b'*' && out[i + 1] == b'/') {
                if out[i] != b'\n' {
                    out[i] = b' ';
                }
                i += 1;
            }
            if i + 1 >= out.len() {
                return bytes.to_vec();
            }
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            continue;
        }
        i += 1;
    }
    in_string = false;
    i = 0;
    while i < out.len() {
        if in_string {
            if out[i] == b'\\' {
                i += 2;
                continue;
            }
            if out[i] == b'"' {
                in_string = false;
            }
        } else if out[i] == b'"' {
            in_string = true;
        } else if out[i] == b',' {
            let next = out[i + 1..]
                .iter()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace());
            if matches!(next, Some(b']' | b'}')) {
                out[i] = b' ';
            }
        }
        i += 1;
    }
    out
}

fn write_parsed_file(
    index: &mut Index,
    relative_path: &str,
    language: &str,
    content_hash: &str,
    parsed: &mct_core::ParsedFile,
) -> Result<usize> {
    let now = unix_now();
    let tx = &index.conn;

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
    let file_scoped: std::collections::HashSet<u32> = parsed.file_scoped.iter().copied().collect();
    for symbol in &parsed.symbols {
        tx.execute(
            "INSERT INTO symbols (file_id, name, kind, parent, line, column, byte_len, end_line, level,
                                  file_scoped)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
                file_scoped.contains(&symbol.id),
            ],
        )?;
        id_map.insert(symbol.id, tx.last_insert_rowid());
    }

    for alias in &parsed.symbol_aliases {
        let Some(&symbol_id) = id_map.get(&alias.symbol) else {
            continue; // Ignore aliases of nonexistent local symbols.
        };
        if alias.name.is_empty() {
            continue;
        }
        tx.execute(
            "INSERT OR IGNORE INTO symbol_aliases (symbol_id, alias) VALUES (?1, ?2)",
            params![symbol_id, alias.name],
        )?;
    }

    for hint in &parsed.entry_points {
        let Some(&symbol_id) = id_map.get(&hint.symbol) else {
            continue; // Ignore hints about nonexistent local symbols.
        };
        if hint.framework.is_empty() || hint.rule.is_empty() || hint.evidence.is_empty() {
            continue;
        }
        let requires = hint.requires.as_ref();
        tx.execute(
            "INSERT OR IGNORE INTO symbol_entry_points
                 (symbol_id, framework, rule, evidence,
                  requires_manifest, requires_package, requires_min_major)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                symbol_id,
                hint.framework,
                hint.rule,
                hint.evidence,
                requires.map(|r| r.manifest_path.as_str()),
                requires.map(|r| r.package.as_str()),
                requires.map(|r| r.min_major),
            ],
        )?;
    }

    for point in &parsed.join_points {
        let Some(&symbol_id) = id_map.get(&point.symbol) else {
            continue;
        };
        let kind = match point.kind {
            mct_core::JoinKind::HttpEndpoint => "http",
        };
        let role = match point.role {
            mct_core::JoinRole::Declares => "declares",
            mct_core::JoinRole::Uses => "uses",
        };
        tx.execute(
            "INSERT INTO join_points (symbol_id, kind, value, role, line, column, byte_len)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT DO NOTHING",
            params![
                symbol_id,
                kind,
                point.value,
                role,
                point.location.line,
                point.location.column,
                point.location.byte_len,
            ],
        )?;
    }

    write_symbol_details(tx, &id_map, &parsed.symbol_details)?;

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
                                    target_kind, target_module, module, member, not_field,
                                    top_level, web_app, include_spec, package)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
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
                target.is_some_and(|t| t.not_field),
                target.is_some_and(|t| t.top_level),
                target.and_then(|t| t.web_app.as_deref()),
                target.and_then(|t| t.include_spec.as_deref()),
                target.and_then(|t| t.package.as_deref()),
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
    Ok(count)
}

/// Local id of the innermost symbol whose span holds `line`: the one that
/// starts last, then ends first, then was emitted last — parsers emit an
/// enclosing symbol before what it contains, so on an identical span (a
/// function that fills its whole file, and the file-level module) the inner
/// one wins. A symbol with no `end_line` spans only its first line.
/// Stores each parsed file's declaration details (ADR-034), normalized. A
/// detail naming no symbol of the file is ignored, and so is any detail after
/// the first for one symbol; the file's previous rows are already gone with
/// its symbols (`ON DELETE CASCADE`).
fn write_symbol_details(
    tx: &rusqlite::Connection,
    id_map: &HashMap<u32, i64>,
    details: &[mct_core::SymbolDetail],
) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for detail in details {
        let Some(&symbol_id) = id_map.get(&detail.symbol) else {
            continue;
        };
        if !seen.insert(symbol_id) {
            continue;
        }
        let detail = detail.clone().normalized();
        let modifiers = (!detail.modifiers.is_empty()).then(|| {
            detail
                .modifiers
                .iter()
                .map(|m| m.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        });
        tx.prepare_cached(
            "INSERT INTO symbol_details
                 (symbol_id, signature, return_type, return_source, visibility,
                  modifiers, generics, parameters_known)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?
        .execute(params![
            symbol_id,
            detail.signature,
            detail.return_type.as_ref().map(|t| t.text.as_str()),
            detail.return_type.as_ref().map(|t| t.provenance.as_str()),
            detail.visibility.map(|v| v.as_str()),
            modifiers,
            detail.generics,
            detail.parameters_known,
        ])?;
        if !detail.parameters_known {
            continue;
        }
        for (position, p) in detail.parameters.iter().enumerate() {
            tx.prepare_cached(
                "INSERT INTO symbol_parameters
                     (symbol_id, position, name, type_text, type_source, default_text, kind)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?
            .execute(params![
                symbol_id,
                position as i64,
                p.name,
                p.type_text.as_ref().map(|t| t.text.as_str()),
                p.type_text.as_ref().map(|t| t.provenance.as_str()),
                p.default,
                p.kind.as_str(),
            ])?;
        }
    }
    Ok(())
}

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
    let tx = &index.conn;
    let seen: std::collections::HashSet<&str> = seen_paths.iter().map(String::as_str).collect();

    let mut stmt = tx.prepare("SELECT relative_path FROM files")?;
    let stored: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    let mut removed = 0;
    for path in &stored {
        if index.exclude.is_excluded(path)
            || (!seen.contains(path.as_str()) && !is_retained(path, unreadable))
        {
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
        if index.exclude.is_excluded(&path)
            || (!seen.contains(path.as_str()) && !is_retained(&path, unreadable))
        {
            tx.execute(
                "DELETE FROM index_issues WHERE relative_path = ?1",
                params![path],
            )?;
        }
    }

    Ok(removed)
}

fn record_issue(
    conn: &rusqlite::Connection,
    relative_path: &str,
    kind: UnsupportedKind,
    detail: &str,
) -> Result<bool> {
    let issue_kind = match kind {
        UnsupportedKind::UnsupportedLanguage => "unsupported_language",
        UnsupportedKind::SyntaxError => "syntax_error",
        UnsupportedKind::ReadFailure => "read_failure",
    };
    // 0 rows: the same issue with the same detail was already stored.
    let changed = conn.execute(
        "INSERT INTO index_issues (relative_path, issue_kind, detail, detected_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(relative_path, issue_kind) DO UPDATE SET
            detail = excluded.detail,
            detected_at = excluded.detected_at
         WHERE index_issues.detail IS NOT excluded.detail",
        params![relative_path, issue_kind, detail, unix_now()],
    )?;
    Ok(changed > 0)
}

/// Publishes the run as a new generation if it wrote anything since
/// `changes_before` (a [`rusqlite::Connection::total_changes`] reading taken
/// after [`symbol_changes::begin`]), if `force` asks for it, or if the index
/// has never been published. Otherwise it writes nothing, so the open
/// transaction commits empty and the WAL is untouched. Either way, records
/// the run as this process's last verification. Returns whether it published.
fn publish_if_changed(
    index: &mut Index,
    changes_before: u64,
    force: bool,
    full_reconciliation: bool,
) -> Result<bool> {
    let changed =
        force || index.conn.total_changes() != changes_before || index.published_generation()? == 0;
    let now = unix_now();
    if changed {
        touch_last_indexed_at(index, full_reconciliation)?;
    }
    index.last_verified_at = Some(now);
    if full_reconciliation {
        index.last_full_verified_at = Some(now);
    }
    Ok(changed)
}

fn touch_last_indexed_at(index: &mut Index, full_reconciliation: bool) -> Result<()> {
    if index.published_generation()? >= i64::MAX as u64 {
        return Err(crate::IndexError::InvalidGenerationMetadata(
            "generation counter exhausted".to_string(),
        ));
    }
    let now = unix_now().to_string();
    index.conn.execute(
        "INSERT INTO index_meta (key, value) VALUES ('last_indexed_at', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![now],
    )?;
    index.conn.execute(
        "INSERT INTO index_meta (key, value) VALUES ('published_generation', '1')
         ON CONFLICT(key) DO UPDATE SET value = CAST(index_meta.value AS INTEGER) + 1",
        [],
    )?;
    if full_reconciliation {
        index.conn.execute(
            "INSERT INTO index_meta (key, value) VALUES ('last_full_reconciled_at', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![now],
        )?;
    }
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
    let published_generation: Option<String> = index
        .conn
        .query_row(
            "SELECT value FROM index_meta WHERE key = 'published_generation'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let last_full_reconciled_at: Option<String> = index
        .conn
        .query_row(
            "SELECT value FROM index_meta WHERE key = 'last_full_reconciled_at'",
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

    let mut stmt = index.conn.prepare(
        "SELECT c.manifest_path,
                (SELECT COUNT(*) FROM ts_paths p WHERE p.manifest_path = c.manifest_path),
                c.error
           FROM ts_configs c ORDER BY c.manifest_path",
    )?;
    let alias_sources: Vec<AliasSource> = stmt
        .query_map([], |row| {
            Ok(AliasSource {
                manifest_path: row.get(0)?,
                aliases: row.get::<_, i64>(1)? as usize,
                error: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    Ok(IndexStatus {
        languages,
        alias_sources,
        total_files: total_files as usize,
        total_symbols: total_symbols as usize,
        last_indexed_at: last_indexed_at.and_then(|v| v.parse().ok()),
        last_verified_at: index.last_verified_at,
        published_generation: crate::parse_published_generation(published_generation)?,
        last_full_reconciled_at: last_full_reconciled_at
            .and_then(|v| v.parse().ok())
            .max(index.last_full_verified_at),
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
        Asset => "asset",
        ModelNode => "model_node",
        Material => "material",
        Animation => "animation",
        Finding => "finding",
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
        Overrides => "overrides",
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

pub(crate) fn to_relative_slash_path(root: &Path, canonical: &Path) -> Option<String> {
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
            ("rockspec_items", "manifest_path"),
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

#[cfg(test)]
mod package_import_tests {
    use super::package_import_mappings;

    #[test]
    fn only_literal_package_relative_imports_are_kept() {
        let json = r##"{"imports":{
            "#a/*":"./src/a/*.js",
            "#b":["./b.ts", "../outside.ts", "src/no-dot.ts"],
            "#c/*":"./c.ts",
            "#d/*/*":"./d/*/*.ts",
            "plain":"./p.ts",
            "#e":{"types":"./e.d.ts","default":"./e/../x.ts"}
        }}"##;
        let pairs = package_import_mappings("web", json);
        let pairs: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("#a/*", "web/src/a/*.js"),
                ("#a/*", "web/src/a/*"),
                ("#b", "web/b.ts"),
                ("#e", "web/e.d.ts"),
            ]
        );
        assert!(package_import_mappings("", "{not json").is_empty());
        assert!(package_import_mappings("", r#"{"name":"x"}"#).is_empty());
    }
}

#[cfg(test)]
mod cmake_include_tests {
    use super::cmake_include_dirs;

    #[test]
    fn only_literal_include_directories_are_kept() {
        let cmake = "# include_directories(commented)\n\
            include_directories(inc \"quoted\" /abs ${VAR}/x)\n\
            target_include_directories(core SYSTEM PUBLIC\n  $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/api>\n  $<INSTALL_INTERFACE:include>\n  ${CMAKE_CURRENT_SOURCE_DIR})\n\
            my_include_directories(not_this)\n";
        assert_eq!(cmake_include_dirs(cmake), ["api", ".", "inc", "quoted"]);
    }
}
