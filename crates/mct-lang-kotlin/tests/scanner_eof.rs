#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, ParseError, SourceFile, SymbolKind};
use mct_lang_kotlin::KotlinParser;

#[test]
fn malformed_annotation_at_eof_returns_a_syntax_error() {
    let contents = String::from_utf8(
        include_bytes!(
            "../fuzz/regressions/parse-kotlin/timeout-71f3392a62d0a0643748aea935f8fbc8e070d37d"
        )
        .to_vec(),
    )
    .unwrap();
    let result = KotlinParser.parse(&SourceFile {
        relative_path: "Fuzz.kt".into(),
        contents,
    });
    assert!(matches!(result, Err(ParseError::Syntax { .. })));
}

#[test]
fn synthetic_newline_preserves_original_source_ranges() {
    for contents in ["", "package demo\nclass Example", "class Example\r"] {
        let parsed = KotlinParser
            .parse(&SourceFile {
                relative_path: "Example.kt".into(),
                contents: contents.into(),
            })
            .unwrap();
        let module = &parsed.symbols[0];
        assert_eq!(module.location.byte_len as usize, contents.len());
        assert_eq!(
            module.location.end_line,
            Some(contents.lines().count().max(1) as u32)
        );
        if contents.contains("class Example") {
            let class = parsed
                .symbols
                .iter()
                .find(|s| s.name == "Example" && s.kind == SymbolKind::Class)
                .unwrap();
            assert_eq!(class.location.byte_len as usize, "class Example".len());
            assert_eq!(
                class.location.line,
                if contents.starts_with("package") {
                    2
                } else {
                    1
                }
            );
        }
    }
}
