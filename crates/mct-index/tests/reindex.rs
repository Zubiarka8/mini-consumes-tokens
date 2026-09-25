//! End-to-end test of the reindex/query pipeline using a trivial fake
//! `LanguageParser` (no tree-sitter grammar), so architecture bugs in
//! `mct-index` are caught before layering a real grammar crate on top.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::Arc;

use mct_core::{
    Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolKind, SymbolRecord,
    SymbolRelation,
};
use mct_index::{ExcludeSet, Index};

/// Toy format: each line is `fn NAME calls OTHER` or `fn NAME`.
///
/// `emit_end_line` stands in for a parser that has just been taught to
/// populate a column it previously left `NULL` — the exact situation the
/// `end_line` column was in when it was added to the schema. Used by
/// `incremental_reindex_does_not_backfill_*` below.
struct FakeParser {
    emit_end_line: bool,
}

impl mct_core::LanguageParser for FakeParser {
    fn language_id(&self) -> &'static str {
        "fake"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["fake"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parsed = ParsedFile::default();
        for (line_no, line) in file.contents.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            if parts.next() != Some("fn") {
                return Err(ParseError::Syntax {
                    path: file.relative_path.clone(),
                    line: line_no as u32 + 1,
                    message: format!("expected `fn`, got `{line}`"),
                });
            }
            let name = parts.next().unwrap_or_default().to_string();
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name,
                kind: SymbolKind::Function,
                location: Location {
                    line: line_no as u32 + 1,
                    column: 1,
                    byte_len: line.len() as u32,
                    end_line: self.emit_end_line.then_some(line_no as u32 + 1),
                },
                parent: None,
                level: None,
            });
            if parts.next() == Some("calls") {
                if let Some(callee) = parts.next() {
                    parsed.relations.push(SymbolRelation {
                        from: id,
                        kind: RelationKind::Calls,
                        to_name: callee.to_string(),
                        location: Location {
                            line: line_no as u32 + 1,
                            column: 1,
                            byte_len: line.len() as u32,
                            end_line: None,
                        },
                    });
                }
            }
        }
        Ok(parsed)
    }
}

fn registry() -> mct_core::LanguageRegistry {
    registry_with(true)
}

fn registry_with(emit_end_line: bool) -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser { emit_end_line }));
    registry
}

#[test]
fn reindex_finds_symbols_calls_and_callers() {
    let dir = tempdir();
    fs::write(dir.join("a.fake"), "fn main calls helper\n").unwrap();
    fs::write(dir.join("b.fake"), "fn helper\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 2);
    assert!(report.issues.is_empty());

    let hits = index.find_symbol("helper").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].relative_path, "b.fake");
    assert_eq!(hits[0].language, "fake");

    let calls = index.find_calls("main").unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].to_name, "helper");

    let callers = index.find_callers("helper").unwrap();
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0].from_symbol, "main");

    let refs = index.find_references("helper").unwrap();
    assert_eq!(refs.len(), 1);
}

/// `Index::generation()` is what `mct-mcp-server::cache::QueryCache` keys
/// its fast-path cache invalidation on — every `reindex()` call, changed
/// files or not, must bump it, since a caller (e.g. a `.mctignore`/exclude
/// change with nothing else different) can alter results without any file
/// content changing.
#[test]
fn generation_is_bumped_by_every_reindex_call_even_a_no_op_one() {
    let dir = tempdir();
    fs::write(dir.join("a.fake"), "fn main\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    assert_eq!(index.generation(), 0);

    index.reindex(&registry(), false).unwrap();
    assert_eq!(index.generation(), 1);

    // A second, no-op incremental reindex (nothing changed on disk) still
    // bumps it — the invalidation signal errs conservative rather than
    // trying to prove nothing could have changed.
    index.reindex(&registry(), false).unwrap();
    assert_eq!(index.generation(), 2);
}

#[test]
fn unchanged_files_are_skipped_on_second_reindex() {
    let dir = tempdir();
    fs::write(dir.join("a.fake"), "fn main\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let first = index.reindex(&registry(), false).unwrap();
    assert_eq!(first.files_parsed, 1);

    let second = index.reindex(&registry(), false).unwrap();
    assert_eq!(second.files_parsed, 0);
    assert_eq!(second.files_unchanged, 1);
}

#[test]
fn deleted_files_are_removed_from_the_index() {
    let dir = tempdir();
    let path = dir.join("a.fake");
    fs::write(&path, "fn main\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert_eq!(index.find_symbol("main").unwrap().len(), 1);

    fs::remove_file(&path).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_removed, 1);
    assert!(index.find_symbol("main").unwrap().is_empty());
}

#[test]
fn syntax_errors_are_reported_without_crashing_the_run() {
    let dir = tempdir();
    fs::write(dir.join("bad.fake"), "not a valid line\n").unwrap();
    fs::write(dir.join("good.fake"), "fn ok\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 1);
    assert_eq!(report.issues.len(), 1);
    assert!(report.issues[0].relative_path.ends_with("bad.fake"));

    let status = index.status().unwrap();
    assert_eq!(status.syntax_errors.len(), 1);
}

#[test]
fn unregistered_extension_is_reported_as_unsupported_language() {
    // "rb" — still genuinely pending (no `mct-lang-ruby` crate exists yet),
    // unlike "java"/"cs"/"go" which this test used before those languages
    // got real crates; `KNOWN_PENDING_LANGUAGES` no longer lists any of them.
    let dir = tempdir();
    fs::write(dir.join("main.rb"), "puts 'hi'\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].detail, "ruby");

    let status = index.status().unwrap();
    assert_eq!(status.unsupported_languages, vec!["ruby".to_string()]);
}

#[test]
fn an_edited_file_is_repicked_up_and_its_stale_symbols_are_dropped() {
    let dir = tempdir();
    let path = dir.join("a.fake");
    fs::write(&path, "fn old_name calls helper\n").unwrap();
    fs::write(dir.join("b.fake"), "fn helper\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert_eq!(index.find_symbol("old_name").unwrap().len(), 1);
    assert_eq!(index.find_callers("helper").unwrap().len(), 1);

    // Rewrite the file: one symbol renamed, the call to `helper` dropped.
    fs::write(&path, "fn new_name\n").unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 1, "only the edited file is re-parsed");
    assert_eq!(report.files_unchanged, 1, "b.fake is untouched");

    assert!(
        index.find_symbol("old_name").unwrap().is_empty(),
        "the renamed-away symbol must not survive an incremental reindex"
    );
    assert_eq!(index.find_symbol("new_name").unwrap().len(), 1);
    assert!(
        index.find_callers("helper").unwrap().is_empty(),
        "the removed call relation must be dropped with its owning symbol"
    );
    // The untouched file's own symbol is still there — a re-parse of one
    // file must not collaterally wipe another's rows.
    assert_eq!(index.find_symbol("helper").unwrap().len(), 1);
}

#[test]
fn an_edit_that_restores_the_previous_content_is_detected_by_hash_not_by_mtime() {
    let dir = tempdir();
    let path = dir.join("a.fake");
    fs::write(&path, "fn one\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();

    fs::write(&path, "fn two\n").unwrap();
    assert_eq!(index.reindex(&registry(), false).unwrap().files_parsed, 1);

    // Back to the original bytes: the content hash matches the *current*
    // stored hash only if the intermediate write was recorded, so this must
    // re-parse rather than be skipped as unchanged.
    fs::write(&path, "fn one\n").unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 1, "content-hash change must be seen both ways");
    assert_eq!(index.find_symbol("one").unwrap().len(), 1);
    assert!(index.find_symbol("two").unwrap().is_empty());
}

#[test]
fn incremental_reindex_does_not_backfill_a_newly_populated_column_on_unchanged_files() {
    // Confirms the behaviour noted for the `end_line` column rollout: a
    // parser that starts populating a column it previously left empty only
    // reaches rows whose *file content* changed. `FakeParser` stands in for
    // the parser upgrade; the file on disk never changes.
    let dir = tempdir();
    fs::write(dir.join("a.fake"), "fn main\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry_with(false), false).unwrap();
    assert_eq!(
        index.find_symbol("main").unwrap()[0].end_line,
        None,
        "precondition: the column starts out NULL"
    );

    // The "upgraded" parser now emits `end_line`, but the file is unchanged.
    let report = index.reindex(&registry_with(true), false).unwrap();
    assert_eq!(report.files_parsed, 0);
    assert_eq!(report.files_unchanged, 1);
    assert_eq!(
        index.find_symbol("main").unwrap()[0].end_line,
        None,
        "incremental reindex leaves the pre-existing NULL in place — it never \
         re-parses a file whose content hash still matches"
    );
}

#[test]
fn a_forced_reindex_does_backfill_a_newly_populated_column_on_unchanged_files() {
    // The other half of the pair above: `force = true` bypasses the content
    // hash check, so the upgraded parser's output does land.
    let dir = tempdir();
    fs::write(dir.join("a.fake"), "fn main\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry_with(false), false).unwrap();
    assert_eq!(index.find_symbol("main").unwrap()[0].end_line, None);

    let report = index.reindex(&registry_with(true), true).unwrap();
    assert_eq!(report.files_parsed, 1, "force re-parses regardless of the hash");
    assert_eq!(report.files_unchanged, 0);
    assert_eq!(
        index.find_symbol("main").unwrap()[0].end_line,
        Some(1),
        "force = true is the documented way to backfill a newly added column"
    );
}

#[test]
fn a_forced_reindex_replaces_rows_instead_of_duplicating_them() {
    let dir = tempdir();
    fs::write(dir.join("a.fake"), "fn main calls helper\n").unwrap();
    fs::write(dir.join("b.fake"), "fn helper\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    let before = index.status().unwrap();

    for _ in 0..3 {
        index.reindex(&registry(), true).unwrap();
    }

    let after = index.status().unwrap();
    assert_eq!(after.total_files, before.total_files);
    assert_eq!(after.total_symbols, before.total_symbols);
    assert_eq!(index.find_symbol("main").unwrap().len(), 1);
    assert_eq!(index.find_callers("helper").unwrap().len(), 1);
}

#[test]
fn relations_table_no_longer_has_a_to_symbol_id_column() {
    // `to_symbol_id` was write-only (populated by an unscoped `SELECT ...
    // LIMIT 1`, never read by any query) and has been dropped — see issue
    // #35. Opens the on-disk database `Index::open` writes to and reads its
    // live schema directly with `rusqlite`, since `Index` itself exposes no
    // schema-introspection API and none is needed for production code.
    let dir = tempdir();
    fs::write(dir.join("a.fake"), "fn main calls helper\n").unwrap();
    fs::write(dir.join("b.fake"), "fn helper\n").unwrap();
    let db_path = dir.join(".mct-index").join("index.sqlite3");

    let mut index = Index::open(&dir, &db_path, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    drop(index); // release the connection so a second one can open the same file

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let mut stmt = conn.prepare("PRAGMA table_info(relations)").unwrap();
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(
        !columns.contains(&"to_symbol_id".to_string()),
        "to_symbol_id should have been dropped by migration: {columns:?}"
    );
    assert!(columns.contains(&"to_name".to_string()), "{columns:?}");
    assert!(columns.contains(&"from_symbol_id".to_string()), "{columns:?}");
}

#[test]
fn relations_resolve_correctly_by_name_even_with_a_duplicate_symbol_name() {
    // Two files each define a symbol named `helper` — exactly the case
    // `to_symbol_id`'s old unscoped `SELECT ... LIMIT 1` resolved
    // arbitrarily (whichever row SQLite returned first, order-dependent).
    // `find_calls`/`find_callers`/`find_references` never read that column,
    // so both definitions must surface and the relation itself must not be
    // silently dropped or deduplicated away.
    let dir = tempdir();
    fs::write(dir.join("a.fake"), "fn main calls helper\n").unwrap();
    fs::write(dir.join("b.fake"), "fn helper\n").unwrap();
    fs::write(dir.join("c.fake"), "fn helper\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();

    let defs = index.find_symbol("helper").unwrap();
    assert_eq!(defs.len(), 2, "both definitions of the duplicate name: {defs:?}");

    let calls = index.find_calls("main").unwrap();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0].to_name, "helper");

    let callers = index.find_callers("helper").unwrap();
    assert_eq!(callers.len(), 1, "{callers:?}");
    assert_eq!(callers[0].from_symbol, "main");
}

/// A fresh temp directory, canonicalized so it matches what `Index::open_in_memory`
/// stores as `root` after its own canonicalization.
fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-index-test-{}", uuid_like()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn uuid_like() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}
