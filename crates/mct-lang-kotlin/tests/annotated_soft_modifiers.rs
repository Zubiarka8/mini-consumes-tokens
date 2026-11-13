//! `@DslMarker annotation class D` and `@X enum class E { A }` (one entry,
//! which reads like a lambda) at file level are declarations, but the pinned grammar reads them as an
//! annotated infix call `annotation class D` with no syntax error. They are
//! indexed like their unannotated spelling, at the original offsets.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, SymbolKind};
use mct_lang_kotlin::KotlinParser;

fn parse(src: &str) -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/Dsl.kt", "Kotlin", src)
}

/// `(kind, name, line, column)` of every symbol but the file module.
fn symbols(parsed: &ParsedFile) -> Vec<(SymbolKind, &str, u32, u32)> {
    parsed
        .symbols
        .iter()
        .skip(1)
        .map(|s| (s.kind, s.name.as_str(), s.location.line, s.location.column))
        .collect()
}

#[test]
fn an_annotated_annotation_class_is_a_class() {
    let parsed = parse("@DslMarker\nannotation class PricingDsl\n\n@PricingDsl\nclass Builder\n");
    assert_eq!(
        symbols(&parsed),
        [
            (SymbolKind::Class, "PricingDsl", 2, 1),
            (SymbolKind::Class, "Builder", 4, 1),
        ]
    );
}

#[test]
fn an_annotated_enum_class_keeps_its_entries() {
    // The declaration starts at its keyword: the annotation is outside the
    // span the grammar can read as a declaration.
    let parsed = parse("@Other\nenum class Color { RED }\n\nclass After\n");
    assert_eq!(
        symbols(&parsed),
        [
            (SymbolKind::Class, "Color", 2, 1),
            (SymbolKind::Field, "RED", 2, 20),
            (SymbolKind::Class, "After", 4, 1),
        ]
    );
}

#[test]
fn an_annotated_infix_call_is_left_alone() {
    // `@Ann a to b` is a real annotated expression; only `annotation class`
    // and `enum class` are declarations.
    let parsed = parse("val pair = @Suppress(\"x\") a to b\n");
    assert!(symbols(&parsed).iter().all(|(_, name, _, _)| *name != "b"));
}
