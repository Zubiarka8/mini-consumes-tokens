//! End-to-end test of the reindex/query pipeline using a trivial fake
//! `LanguageParser` (no tree-sitter grammar), so architecture bugs in
//! `ccm-index` are caught before layering a real grammar crate on top.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/ccm-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::Arc;

use ccm_core::{
    Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolKind, SymbolRecord,
    SymbolRelation,
};
use ccm_index::{ExcludeSet, Index};

/// Toy format: each line is `fn NAME calls OTHER` or `fn NAME`.
struct FakeParser;

impl ccm_core::LanguageParser for FakeParser {
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
                    end_line: Some(line_no as u32 + 1),
                },
                parent: None,
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

fn registry() -> ccm_core::LanguageRegistry {
    let mut registry = ccm_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
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
    // "rb" — still genuinely pending (no `ccm-lang-ruby` crate exists yet),
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

/// A fresh temp directory, canonicalized so it matches what `Index::open_in_memory`
/// stores as `root` after its own canonicalization.
fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccm-index-test-{}", uuid_like()));
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
