//! A field initializer runs code like a method body: the calls in
//! `List<String> names = List.of("a")`, `int size = helper()`, and the
//! methods of an anonymous class in `Runnable task = new Runnable() { … }`
//! are indexed like those in an initializer block.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_java::JavaParser;

const REGISTRY: &str = r#"package app;

import java.util.List;

public class Registry {
    private final List<String> names = List.of("a");
    private final Runnable task = new Runnable() {
        @Override
        public void run() {
            helper();
        }
    };
    private final int size = helper(), count;

    private static int helper() {
        return 0;
    }
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JavaParser, "app/Registry.java", "Java", REGISTRY)
}

#[test]
fn calls_in_a_field_initializer_are_recorded() {
    let parsed = parsed();
    let calls: Vec<(&str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| (r.to_name.as_str(), r.location.line))
        .collect();
    assert_eq!(
        calls,
        [("of", 6), ("Runnable", 7), ("helper", 10), ("helper", 13)]
    );
}

#[test]
fn an_anonymous_class_in_a_field_initializer_has_its_methods() {
    let parsed = parsed();
    let methods: Vec<&str> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Method)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(methods, ["run", "helper"]);
    let fields: Vec<&str> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Field)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(fields, ["names", "task", "size", "count"]);
}
