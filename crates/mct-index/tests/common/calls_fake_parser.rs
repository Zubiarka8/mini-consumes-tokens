//! A toy line-per-symbol parser (`NAME` or `NAME calls OTHER`) for the
//! `mct-index` integration tests that need call relations without a real
//! grammar. Included with `#[path]`, like `fake_parser.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{
    Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolKind, SymbolRecord,
    SymbolRelation,
};

/// Each line is `NAME` or `NAME calls OTHER`.
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
        for (n, line) in file.contents.lines().enumerate() {
            let mut parts = line.split_whitespace();
            let Some(name) = parts.next() else { continue };
            let location = Location {
                line: n as u32 + 1,
                column: 1,
                byte_len: line.len() as u32,
                end_line: Some(n as u32 + 1),
            };
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name: name.to_string(),
                kind: SymbolKind::Function,
                location,
                parent: None,
                level: None,
            });
            if let (Some("calls"), Some(callee)) = (parts.next(), parts.next()) {
                parsed.relations.push(SymbolRelation {
                    from: id,
                    kind: RelationKind::Calls,
                    to_name: callee.to_string(),
                    location,
                });
            }
        }
        Ok(parsed)
    }
}

/// A registry holding only the toy parser.
pub fn registry() -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    registry
}
