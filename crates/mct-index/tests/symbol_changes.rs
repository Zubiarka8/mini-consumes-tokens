//! `Index::track_symbol_changes`: while on, a reindex names the symbols it
//! added and removed (netted out per file), whatever path removed them — an
//! edited file, a deleted file or a full rescan. Off, the counts stay zero.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::{Location, ParseError, ParsedFile, SourceFile, SymbolKind, SymbolRecord};
use mct_index::{ExcludeSet, Index};

/// Toy format: each non-empty line is `fn NAME`; anything else is a syntax error.
struct FakeParser;

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
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name: parts.next().unwrap_or_default().to_string(),
                kind: SymbolKind::Function,
                location: Location {
                    line: line_no as u32 + 1,
                    column: 1,
                    byte_len: line.len() as u32,
                    end_line: Some(line_no as u32 + 1),
                },
                parent: None,
                level: None,
            });
        }
        Ok(parsed)
    }
}

fn registry() -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    registry
}

fn tempdir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-symbol-changes-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn write(root: &Path, rel: &str, contents: &str) -> PathBuf {
    let path = root.join(rel);
    fs::write(&path, contents).unwrap();
    path
}

/// A tracked index of `a.fake` (`one`, `extra`) and `b.fake` (`keep`, `gone`).
fn tracked(root: &Path) -> Index {
    write(root, "a.fake", "fn one\nfn extra\n");
    write(root, "b.fake", "fn keep\nfn gone\n");
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.track_symbol_changes(true).unwrap();
    index.reindex(&registry(), false).unwrap();
    index
}

#[test]
fn a_function_deleted_from_a_file_is_named() {
    let root = tempdir();
    let mut index = tracked(&root);
    let a = write(&root, "a.fake", "fn one\n");

    let report = index.reindex_paths(&registry(), &[a]).unwrap();

    assert_eq!(report.files_parsed, 1);
    assert_eq!(report.symbols_removed, 1);
    assert_eq!(report.symbols_added, 0);
    assert_eq!(report.removed_symbols, ["function extra (a.fake)"]);
}

#[test]
fn a_function_added_to_a_file_is_counted() {
    let root = tempdir();
    let mut index = tracked(&root);
    let a = write(&root, "a.fake", "fn one\nfn extra\nfn fresh\n");

    let report = index.reindex_paths(&registry(), &[a]).unwrap();

    assert_eq!(report.symbols_added, 1);
    assert_eq!(report.symbols_removed, 0);
    assert!(report.removed_symbols.is_empty());
}

#[test]
fn rewriting_a_file_without_changing_its_symbols_reports_nothing() {
    let root = tempdir();
    let mut index = tracked(&root);
    // Same functions, different content (a hash change forces a re-parse).
    let a = write(&root, "a.fake", "\nfn one\n\nfn extra\n");

    let report = index.reindex_paths(&registry(), &[a]).unwrap();

    assert_eq!(report.files_parsed, 1);
    assert_eq!(report.symbols_added, 0);
    assert_eq!(report.symbols_removed, 0);
}

#[test]
fn a_deleted_file_names_all_of_its_symbols() {
    let root = tempdir();
    let mut index = tracked(&root);
    let b = root.join("b.fake");
    fs::remove_file(&b).unwrap();

    let report = index.reindex_paths(&registry(), &[b]).unwrap();

    assert_eq!(report.files_removed, 1);
    assert_eq!(report.symbols_removed, 2);
    assert_eq!(
        report.removed_symbols,
        ["function gone (b.fake)", "function keep (b.fake)"]
    );
}

#[test]
fn a_full_reindex_reports_removals_too() {
    let root = tempdir();
    let mut index = tracked(&root);
    write(&root, "a.fake", "fn one\n");
    fs::remove_file(root.join("b.fake")).unwrap();

    let report = index.reindex(&registry(), false).unwrap();

    assert_eq!(report.symbols_removed, 3);
    assert_eq!(report.symbols_added, 0);
}

#[test]
fn a_file_that_stops_parsing_loses_its_symbols() {
    let root = tempdir();
    let mut index = tracked(&root);
    let a = write(&root, "a.fake", "this is not valid\n");

    let report = index.reindex_paths(&registry(), &[a]).unwrap();

    assert_eq!(report.symbols_removed, 2);
}

#[test]
fn counts_start_fresh_on_every_run() {
    let root = tempdir();
    let mut index = tracked(&root);
    let a = write(&root, "a.fake", "fn one\n");
    index.reindex_paths(&registry(), &[a]).unwrap();

    let report = index.reindex(&registry(), false).unwrap();

    assert_eq!(report.symbols_removed, 0);
    assert_eq!(report.symbols_added, 0);
}

#[test]
fn nothing_is_counted_unless_tracking_is_on() {
    let root = tempdir();
    write(&root, "a.fake", "fn one\nfn extra\n");
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    let a = write(&root, "a.fake", "fn one\n");

    let report = index.reindex_paths(&registry(), &[a]).unwrap();

    assert_eq!(report.symbols_removed, 0);
    assert!(report.removed_symbols.is_empty());
}

#[test]
fn tracking_can_be_turned_off_again() {
    let root = tempdir();
    let mut index = tracked(&root);
    index.track_symbol_changes(false).unwrap();
    let a = write(&root, "a.fake", "fn one\n");

    let report = index.reindex_paths(&registry(), &[a]).unwrap();

    assert_eq!(report.symbols_removed, 0);
}
