//! A `#define` is a symbol: an object-like macro (`#define MAX 4`) is a
//! `constant` and a function-like one (`#define SQ(x) …`) a `function`, as
//! Rust's `macro_rules!` is, so `SQ(MAX)` finds its definition. The
//! preprocessor knows no namespace or class, so a macro has no parent
//! wherever it is written. An include guard (`#ifndef SHOP_H` then
//! `#define SHOP_H` with no value) only stops a second inclusion and is no
//! symbol.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_cpp::CppParser;

const SOURCE: &str = r#"#ifndef SHOP_H
#define SHOP_H
#define MAX 4
#define SQ(x) ((x) * (x))
#define DEBUG
namespace shop {
#define LIMIT 9
int area() { return SQ(MAX); }
}
#endif
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CppParser, "src/shop.hpp", "C++", SOURCE)
}

#[test]
fn a_define_is_a_constant_or_a_function_with_no_parent() {
    let parsed = parsed();
    let macros: Vec<(String, SymbolKind, Option<String>, u32)> = parsed
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Constant | SymbolKind::Function))
        .filter(|s| s.name != "area")
        .map(|s| (s.name.clone(), s.kind, s.parent.clone(), s.location.line))
        .collect();
    assert_eq!(
        macros,
        [
            ("MAX".to_string(), SymbolKind::Constant, None, 3),
            ("SQ".to_string(), SymbolKind::Function, None, 4),
            ("DEBUG".to_string(), SymbolKind::Constant, None, 5),
            ("LIMIT".to_string(), SymbolKind::Constant, None, 7),
        ]
    );
}

#[test]
fn an_include_guard_is_no_symbol() {
    assert!(parsed().symbols.iter().all(|s| s.name != "SHOP_H"));
}

#[test]
fn a_function_like_macro_is_called_by_name() {
    let parsed = parsed();
    assert!(parsed
        .relations
        .iter()
        .any(|r| r.kind == RelationKind::Calls && r.to_name == "SQ"));
}
