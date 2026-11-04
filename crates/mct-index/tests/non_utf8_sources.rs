//! A source file that is not valid UTF-8 cannot be parsed. Text in another
//! encoding (a Latin-1 PHP file, a Windows-1252 script) must show up as an
//! issue instead of vanishing from the index without a trace; a binary file
//! that merely carries a source extension stays out of the report.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::{Location, ParseError, ParsedFile, SourceFile, SymbolKind, SymbolRecord};
use mct_index::{ExcludeSet, Index, UnsupportedKind};

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
            let Some(name) = line.trim().strip_prefix("fn ") else {
                continue;
            };
            parsed.symbols.push(SymbolRecord {
                id: parsed.symbols.len() as u32,
                name: name.trim().to_string(),
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

fn tempdir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-non-utf8-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn write(root: &Path, rel: &str, contents: &[u8]) {
    fs::write(root.join(rel), contents).unwrap();
}

fn index(root: &Path) -> (Index, mct_core::LanguageRegistry) {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    (
        Index::open_in_memory(root, ExcludeSet::default()).unwrap(),
        registry,
    )
}

#[test]
fn latin1_text_is_reported_not_silently_dropped() {
    let root = tempdir();
    write(&root, "good.fake", b"fn kept\n");
    // "fn caf\xE9": Latin-1 `é`, an invalid UTF-8 sequence, on line 2.
    write(&root, "legacy.fake", b"fn first\nfn caf\xE9\n");
    let (mut index, registry) = index(&root);

    let report = index.reindex(&registry, false).unwrap();

    assert_eq!(report.files_parsed, 1);
    let [issue] = report.issues.as_slice() else {
        panic!("expected exactly the encoding issue: {:?}", report.issues);
    };
    assert_eq!(issue.relative_path, "legacy.fake");
    assert_eq!(issue.kind, UnsupportedKind::SyntaxError);
    assert!(
        issue.detail.contains("line 2") && issue.detail.contains("UTF-8"),
        "{}",
        issue.detail
    );
    assert!(index.find_symbol("first").unwrap().is_empty());
    assert_eq!(index.find_symbol("kept").unwrap().len(), 1);
}

#[test]
fn the_issue_is_kept_on_a_second_run_and_cleared_once_the_file_is_utf8() {
    let root = tempdir();
    write(&root, "legacy.fake", b"fn caf\xE9\n");
    let (mut index, registry) = index(&root);
    index.reindex(&registry, false).unwrap();

    let again = index.reindex(&registry, false).unwrap();
    assert_eq!(again.issues.len(), 1, "{:?}", again.issues);

    write(&root, "legacy.fake", "fn café\n".as_bytes());
    let fixed = index.reindex(&registry, false).unwrap();
    assert!(fixed.issues.is_empty(), "{:?}", fixed.issues);
    assert_eq!(index.find_symbol("café").unwrap().len(), 1);
}

#[test]
fn a_file_that_turns_into_latin1_loses_its_stale_symbols() {
    let root = tempdir();
    write(&root, "legacy.fake", b"fn stale\n");
    let (mut index, registry) = index(&root);
    index.reindex(&registry, false).unwrap();
    assert_eq!(index.find_symbol("stale").unwrap().len(), 1);

    write(&root, "legacy.fake", b"fn stale\nfn caf\xE9\n");
    let report = index.reindex(&registry, false).unwrap();

    assert_eq!(report.issues.len(), 1);
    assert!(index.find_symbol("stale").unwrap().is_empty());
}

#[test]
fn a_binary_file_with_a_source_extension_is_not_reported() {
    let root = tempdir();
    write(&root, "blob.fake", b"fn x\0\xFF\xFE binary");
    let (mut index, registry) = index(&root);

    let report = index.reindex(&registry, false).unwrap();

    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!(report.files_parsed, 0);
}
