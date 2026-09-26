// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-rust/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_rust::RustParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    RustParser
        .parse(&SourceFile {
            relative_path: "src/lib.rs".to_string(),
            contents: src.to_string(),
        })
        .expect("valid Rust source should parse")
}

#[test]
fn extracts_function_and_call() {
    let parsed = parse(
        r#"
        fn helper() -> i32 { 42 }
        fn main() {
            let x = helper();
            println!("{x}");
        }
        "#,
    );
    let names: Vec<_> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"helper"));
    assert!(names.contains(&"main"));

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"helper"));
}

#[test]
fn extracts_generic_struct_trait_and_impl_methods() {
    let parsed = parse(
        r#"
        trait Shape {
            fn area(&self) -> f64;
        }

        struct Rect<T> {
            width: T,
            height: T,
        }

        impl<T> Shape for Rect<T> {
            fn area(&self) -> f64 {
                0.0
            }
        }
        "#,
    );

    let struct_sym = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Rect")
        .expect("Rect struct should be indexed");
    assert_eq!(struct_sym.kind, SymbolKind::Struct);

    let trait_sym = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Shape")
        .expect("Shape trait should be indexed");
    assert_eq!(trait_sym.kind, SymbolKind::Trait);

    let methods: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Method && s.name == "area")
        .collect();
    // one from the trait's signature, one from the impl block's body
    assert_eq!(methods.len(), 2);
    assert!(methods.iter().any(|m| m.parent.as_deref() == Some("Shape")));
    assert!(methods
        .iter()
        .any(|m| m.parent.as_deref() == Some("Rect<T>")));

    let implements: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Implements)
        .collect();
    assert!(implements.iter().any(|r| r.to_name == "Shape"));
}

#[test]
fn extracts_use_imports() {
    let parsed = parse("use std::collections::HashMap;\nuse std::fmt::{Debug, Display};\n");
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"HashMap"));
    assert!(imports.contains(&"Debug"));
    assert!(imports.contains(&"Display"));
}

#[test]
fn function_end_line_spans_the_whole_multiline_body() {
    let parsed = parse("fn multiline() -> i32 {\n    let x = 1;\n    let y = 2;\n    x + y\n}\n");
    let sym = parsed
        .symbols
        .iter()
        .find(|s| s.name == "multiline")
        .expect("multiline function should be indexed");
    // Line 1: `fn multiline() -> i32 {`, line 5: the closing `}`.
    assert_eq!(sym.location.line, 1);
    assert_eq!(sym.location.end_line, Some(5));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = RustParser.parse(&SourceFile {
        relative_path: "src/broken.rs".to_string(),
        contents: "fn main( { this is not valid rust @@@".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}

fn literal_texts(parsed: &mct_core::ParsedFile) -> Vec<(&str, u32)> {
    parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect()
}

#[test]
fn prose_literals_keep_only_fixed_format_fragments() {
    let parsed = parse(
        r#"
fn connect(url: &str) -> Result<(), String> {
    Err(format!("Error de conexión con la BD: {url} (retry {}/{} failed)", 1, 3))
}
"#,
    );
    assert_eq!(
        literal_texts(&parsed),
        vec![("Error de conexión con la BD:", 3)],
        "`(retry `, `/` and ` failed)` are too short to be prose"
    );
}

#[test]
fn literals_resolve_escapes_and_skip_non_prose() {
    let parsed = parse(
        r####"
const KEY: &str = "relative_path";
const TOKEN: &str = "Xk9mQ2vR7tLp4wZs8bNc3hJd";
fn run() {
    log("first line\nsecond \"quoted\" line");
    log(r"raw \d+ pattern kept as written");
    log("could not open the file");
    log("could not open the file");
    log("braces {{escaped}} stay in text");
}
"####,
    );
    assert_eq!(
        literal_texts(&parsed),
        vec![
            ("first line second \"quoted\" line", 5),
            (r"raw \d+ pattern kept as written", 6),
            ("could not open the file", 7),
            ("braces {escaped} stay in text", 9),
        ]
    );
}

#[test]
fn a_multiline_literal_fragment_reports_the_line_it_starts_on() {
    let parsed = parse("fn f() {\n    let _ = \"{}\nthe second fragment starts here\";\n}\n");
    assert_eq!(
        literal_texts(&parsed),
        vec![("the second fragment starts here", 3)]
    );
}
