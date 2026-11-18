//! A record's compact constructor (`public Point { … }`) is a constructor of
//! the record, like an explicit one: a `method` named after the record, with
//! the calls of its body attributed to it, not to the file's module.

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_java::JavaParser;

const SOURCE: &str = r#"public record Point(int x, int y) {
    public Point {
        if (x < 0) throw new IllegalArgumentException();
        check(y);
    }
    public int sum() { return x + y; }
}
"#;

#[test]
fn a_compact_constructor_is_a_method_of_its_record() {
    let parsed = mct_corpus::parse_source(&JavaParser, "Point.java", "Java", SOURCE);
    let ctor = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Point" && s.kind == SymbolKind::Method)
        .expect("compact constructor");
    assert_eq!(ctor.parent.as_deref(), Some("Point"));
    assert_eq!(ctor.location.line, 2);
}

#[test]
fn the_calls_of_its_body_belong_to_it() {
    let parsed = mct_corpus::parse_source(&JavaParser, "Point.java", "Java", SOURCE);
    let owner = |name: &str| {
        let call = parsed
            .relations
            .iter()
            .find(|r| r.kind == RelationKind::Calls && r.to_name == name)
            .unwrap();
        let from = parsed.symbols.iter().find(|s| s.id == call.from).unwrap();
        (from.name.clone(), from.kind)
    };
    assert_eq!(
        owner("IllegalArgumentException"),
        ("Point".to_string(), SymbolKind::Method)
    );
    assert_eq!(owner("check"), ("Point".to_string(), SymbolKind::Method));
}

#[test]
fn a_record_without_one_has_no_constructor_symbol() {
    let parsed = mct_corpus::parse_source(
        &JavaParser,
        "P.java",
        "Java",
        "public record P(int x) { int twice() { return x * 2; } }\n",
    );
    assert!(!parsed
        .symbols
        .iter()
        .any(|s| s.name == "P" && s.kind == SymbolKind::Method));
}
