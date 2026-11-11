//! Declaration details (ADR-034, issue #321) through parse -> SQLite -> read:
//! stored per symbol, replaced on reparse, removed with the file, and never
//! invented for a parser that emits none.
//!
//! Toy parser, one symbol per line: `def NAME [detail]`. A line with
//! `detail` gets a fixed detail; `orphan` adds one naming no symbol.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use mct_core::{
    Location, Modifier, Parameter, ParameterKind, ParseError, ParsedFile, SourceFile, SymbolDetail,
    SymbolKind, SymbolRecord, TypeText, Visibility,
};
use mct_index::{ExcludeSet, Index};

struct FakeParser;

impl mct_core::LanguageParser for FakeParser {
    fn language_id(&self) -> &'static str {
        "alpha"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["a"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parsed = ParsedFile::default();
        for (line_no, line) in file.contents.lines().enumerate() {
            let words: Vec<&str> = line.split_whitespace().collect();
            if words.first() == Some(&"orphan") {
                parsed.symbol_details.push(SymbolDetail {
                    symbol: 999,
                    signature: Some("fn ghost()".into()),
                    ..Default::default()
                });
                continue;
            }
            let Some((&"def", rest)) = words.split_first() else {
                continue;
            };
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name: rest[0].to_string(),
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
            if rest.get(1) == Some(&"detail") {
                parsed.symbol_details.push(SymbolDetail {
                    symbol: id,
                    signature: Some(format!(
                        "pub async fn {}(\n    &self,\n    n: u8,\n) -> u8",
                        rest[0]
                    )),
                    parameters: vec![
                        Parameter {
                            name: Some("self".into()),
                            type_text: None,
                            default: None,
                            kind: ParameterKind::Receiver,
                        },
                        Parameter {
                            name: Some("n".into()),
                            type_text: Some(TypeText::syntax("u8")),
                            default: Some("1".into()),
                            kind: ParameterKind::Positional,
                        },
                    ],
                    parameters_known: true,
                    return_type: Some(TypeText::syntax("u8")),
                    visibility: Some(Visibility::Public),
                    modifiers: vec![Modifier::Unsafe, Modifier::Async],
                    generics: Some("<T>".into()),
                });
            }
        }
        Ok(parsed)
    }
}

fn index_of(files: &[(&str, &str)]) -> (PathBuf, Index, mct_core::LanguageRegistry) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-index-symbol-details-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for (path, contents) in files {
        fs::write(dir.join(path), contents).unwrap();
    }
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    (dir, index, registry)
}

#[test]
fn a_detail_is_stored_normalized_and_read_back_by_symbol() {
    let (dir, index, _) = index_of(&[("x.a", "def plain\ndef run detail\n")]);
    let detail = index.symbol_details("x.a", 2, "run").unwrap().unwrap();
    assert_eq!(
        detail.signature.as_deref(),
        Some("pub async fn run( &self, n: u8, ) -> u8")
    );
    assert_eq!(detail.return_type.as_deref(), Some("u8"));
    assert_eq!(detail.return_source.as_deref(), Some("syntax"));
    assert_eq!(detail.visibility.as_deref(), Some("public"));
    assert_eq!(detail.modifiers, ["async", "unsafe"]);
    assert_eq!(detail.generics.as_deref(), Some("<T>"));
    assert!(detail.parameters_known);
    let params: Vec<_> = detail
        .parameters
        .iter()
        .map(|p| {
            format!(
                "{}:{}:{}:{}",
                p.name.as_deref().unwrap_or("_"),
                p.type_text.as_deref().unwrap_or("?"),
                p.default_text.as_deref().unwrap_or("-"),
                p.kind
            )
        })
        .collect();
    assert_eq!(params, ["self:?:-:receiver", "n:u8:1:positional"]);
    // A symbol without a detail has no row, not an empty one.
    assert!(index.symbol_details("x.a", 1, "plain").unwrap().is_none());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_detail_naming_no_symbol_is_ignored() {
    let (dir, index, _) = index_of(&[("x.a", "orphan\ndef run\n")]);
    assert!(index.symbol_details("x.a", 2, "run").unwrap().is_none());
    assert_eq!(index.symbol_detail_count().unwrap(), 0);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reparsing_a_file_replaces_its_details_and_deleting_it_removes_them() {
    let (dir, mut index, registry) = index_of(&[("x.a", "def run detail\n")]);
    assert_eq!(index.symbol_detail_count().unwrap(), 1);
    fs::write(dir.join("x.a"), "def run\n").unwrap();
    index.reindex(&registry, false).unwrap();
    assert_eq!(index.symbol_detail_count().unwrap(), 0);
    fs::write(dir.join("x.a"), "def run detail\n").unwrap();
    index.reindex(&registry, false).unwrap();
    assert_eq!(index.symbol_detail_count().unwrap(), 1);
    fs::remove_file(dir.join("x.a")).unwrap();
    index.reindex(&registry, false).unwrap();
    assert_eq!(index.symbol_detail_count().unwrap(), 0);
    fs::remove_dir_all(dir).unwrap();
}
