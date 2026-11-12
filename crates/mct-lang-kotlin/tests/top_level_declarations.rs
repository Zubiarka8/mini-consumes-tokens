//! A file-level declaration is a symbol with no parent: `typealias` is a
//! `type_alias`, `const val` a `constant`, and any other `val`/`var` a
//! `variable` (each name of `val (a, b) = …` too). A property inside a
//! function is a local and no symbol; one inside a class stays a `field`.
//! An extension property (`val Cart.empty`) is a `field` of its receiver
//! type, as an extension function is a method of it.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_kotlin::KotlinParser;

const SOURCE: &str = r#"package shop

typealias Ids = List<Long>
typealias Same<A> = Pair<A, A>
const val MAX = 4
val greeting: String = make()
private var count = 0
val (low, high) = 1 to 2

fun f() {
    val local = 1
}

class Cart {
    val size = 0
}

val Cart.empty: Boolean
    get() = size == 0
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/shop/Top.kt", "Kotlin", SOURCE)
}

#[test]
fn a_file_level_declaration_is_a_parentless_symbol() {
    let parsed = parsed();
    let symbols: Vec<(String, SymbolKind, Option<String>, u32)> = parsed
        .symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::TypeAlias
                    | SymbolKind::Constant
                    | SymbolKind::Variable
                    | SymbolKind::Field
            )
        })
        .map(|s| (s.name.clone(), s.kind, s.parent.clone(), s.location.line))
        .collect();
    assert_eq!(
        symbols,
        [
            ("Ids".to_string(), SymbolKind::TypeAlias, None, 3),
            ("Same".to_string(), SymbolKind::TypeAlias, None, 4),
            ("MAX".to_string(), SymbolKind::Constant, None, 5),
            ("greeting".to_string(), SymbolKind::Variable, None, 6),
            ("count".to_string(), SymbolKind::Variable, None, 7),
            ("low".to_string(), SymbolKind::Variable, None, 8),
            ("high".to_string(), SymbolKind::Variable, None, 8),
            (
                "size".to_string(),
                SymbolKind::Field,
                Some("Cart".to_string()),
                15
            ),
            (
                "empty".to_string(),
                SymbolKind::Field,
                Some("Cart".to_string()),
                18
            ),
        ]
    );
}

#[test]
fn a_file_level_initializer_still_calls() {
    let parsed = parsed();
    assert!(parsed
        .relations
        .iter()
        .any(|r| r.kind == RelationKind::Calls && r.to_name == "make"));
}
