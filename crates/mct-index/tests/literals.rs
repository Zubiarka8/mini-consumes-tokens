//! String-literal indexing end to end, with a fake `LanguageParser`: literals
//! reach `literals_fts`, are attributed to their enclosing symbol, match
//! exact phrases diacritic-insensitively, and leave no stale FTS rows when
//! their file changes or disappears.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use mct_core::{
    LanguageRegistry, LiteralCollector, Location, ParseError, ParsedFile, SourceFile, SymbolKind,
    SymbolRecord,
};
use mct_index::{ExcludeSet, Index, QueryScope};

/// Toy format: `fn NAME` opens a function that runs until the next `fn`;
/// `say TEXT` is a string literal holding TEXT.
struct FakeParser;

impl mct_core::LanguageParser for FakeParser {
    fn language_id(&self) -> &'static str {
        "fake"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["fake"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut symbols: Vec<SymbolRecord> = Vec::new();
        let mut literals = LiteralCollector::default();
        let last_line = file.contents.lines().count() as u32;
        for (i, line) in file.contents.lines().enumerate() {
            let line_no = i as u32 + 1;
            if let Some(name) = line.trim().strip_prefix("fn ") {
                if let Some(prev) = symbols.last_mut() {
                    prev.location.end_line = Some(line_no - 1);
                }
                symbols.push(SymbolRecord {
                    id: symbols.len() as u32,
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    location: Location {
                        line: line_no,
                        column: 1,
                        byte_len: line.len() as u32,
                        end_line: Some(last_line),
                    },
                    parent: None,
                    level: None,
                });
            } else if let Some(text) = line.trim().strip_prefix("say ") {
                literals.push(text, line_no);
            }
        }
        Ok(ParsedFile {
            symbols,
            literals: literals.finish(),
            ..Default::default()
        })
    }
}

fn tempdir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("mct-literals-{tag}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    registry
}

fn found(index: &Index, phrase: &str) -> Vec<(String, u32, Option<String>, String)> {
    index
        .search_literals(phrase, QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| (h.relative_path, h.line, h.symbol.map(|(name, _)| name), h.text))
        .collect()
}

#[test]
fn a_phrase_finds_its_literal_with_line_and_enclosing_symbol() {
    let root = tempdir("phrase");
    fs::write(
        root.join("db.fake"),
        "say top level message here\nfn connect\nsay Error de conexión con la BD:\nfn other\n",
    )
    .unwrap();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();

    let expected = vec![(
        "db.fake".to_string(),
        3,
        Some("connect".to_string()),
        "Error de conexión con la BD:".to_string(),
    )];
    assert_eq!(found(&index, "Error de conexión con la BD"), expected);
    // Case, diacritics and trailing punctuation don't matter.
    assert_eq!(found(&index, "error de CONEXION con la bd:"), expected);
    // A literal outside every symbol has none.
    assert_eq!(found(&index, "top level message")[0].2, None);
    // Words must be consecutive and in order.
    assert!(found(&index, "conexión la BD").is_empty());
    assert!(found(&index, "BD la con").is_empty());
    // Nothing alphanumeric: no match, no error.
    assert!(found(&index, "::").is_empty());
}

#[test]
fn literals_follow_their_file_through_edits_and_deletion() {
    let root = tempdir("lifecycle");
    let file = root.join("a.fake");
    fs::write(&file, "fn connect\nsay Error de conexión con la BD\n").unwrap();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert_eq!(found(&index, "conexión con la").len(), 1);

    fs::write(&file, "fn connect\nsay Could not reach the database\n").unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(found(&index, "conexión con la").is_empty());
    assert_eq!(found(&index, "reach the database").len(), 1);

    // Deleting the file cascades to its literals. A new literal then reuses
    // the freed rowid; a stale FTS entry would make the old phrase match it.
    fs::remove_file(&file).unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(found(&index, "reach the database").is_empty());
    fs::write(root.join("b.fake"), "say something else entirely\n").unwrap();
    index.reindex(&registry(), false).unwrap();
    assert!(found(&index, "reach the database").is_empty());
    assert_eq!(found(&index, "something else").len(), 1);
}
