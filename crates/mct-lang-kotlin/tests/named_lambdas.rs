//! A property bound to a lambda (`val twice = { x: Int -> … }`) or an
//! anonymous function (`val add = fun(a: Int, b: Int): Int { … }`) is a
//! `function`, as a C++ `auto f = [](…) { … }` or a Rust `let f = |x| …`
//! is: top-level it has no parent, like a top-level `fun`; in a function
//! body it is owned by that function, so `local()` resolves. The calls in
//! its body are its own. A class property holding a lambda stays a
//! `field`, and so do a delegated property (`by lazy { … }`), an extension
//! property and a destructuring declaration.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_kotlin::KotlinParser;

const SOURCE: &str = r#"package a

val twice = { x: Int -> helper() * x }
val add: (Int, Int) -> Int = fun(a: Int, b: Int): Int { return a + b }
val lazyOne by lazy { helper() }
fun helper(): Int = 1
class S {
  val cb = { helper() }
}
fun run() {
  val local = { helper() }
  local()
  listOf(1).map { it + helper() }
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/A.kt", "Kotlin", SOURCE)
}

#[test]
fn a_property_bound_to_a_lambda_is_a_function() {
    let parsed = parsed();
    let symbols: Vec<(&str, SymbolKind, Option<&str>)> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind != SymbolKind::Module)
        .map(|s| (s.name.as_str(), s.kind, s.parent.as_deref()))
        .collect();
    assert_eq!(
        symbols,
        [
            ("twice", SymbolKind::Function, None),
            ("add", SymbolKind::Function, None),
            ("lazyOne", SymbolKind::Variable, None),
            ("helper", SymbolKind::Function, None),
            ("S", SymbolKind::Class, None),
            ("cb", SymbolKind::Field, Some("S")),
            ("run", SymbolKind::Function, None),
            ("local", SymbolKind::Function, Some("run")),
        ]
    );
    let detail = |name: &str| {
        let id = parsed.symbols.iter().find(|s| s.name == name).unwrap().id;
        parsed
            .symbol_details
            .iter()
            .find(|d| d.symbol == id)
            .unwrap()
            .clone()
    };
    let twice = detail("twice");
    assert_eq!(twice.signature.as_deref(), Some("val twice = { x: Int ->"));
    let params: Vec<(Option<&str>, Option<&str>)> = twice
        .parameters
        .iter()
        .map(|p| {
            (
                p.name.as_deref(),
                p.type_text.as_ref().map(|t| t.text.as_str()),
            )
        })
        .collect();
    assert_eq!(params, [(Some("x"), Some("Int"))]);
    let add = detail("add");
    assert_eq!(
        add.signature.as_deref(),
        Some("val add: (Int, Int) -> Int = fun(a: Int, b: Int): Int")
    );
    assert_eq!(add.parameters.len(), 2);
    let local = detail("local");
    assert_eq!(local.signature.as_deref(), Some("val local"));
    assert!(!local.parameters_known, "a bare lambda may take `it`");
}

#[test]
fn the_calls_in_its_body_are_its_own() {
    let parsed = parsed();
    let name_of = |id| {
        parsed
            .symbols
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .name
            .as_str()
    };
    let mut calls: Vec<(&str, &str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| (name_of(r.from), r.to_name.as_str(), r.location.line))
        .collect();
    calls.sort_by_key(|c| (c.2, c.1));
    assert_eq!(
        calls,
        [
            ("twice", "helper", 3),
            ("A", "helper", 5),
            ("A", "lazy", 5),
            ("S", "helper", 8),
            ("local", "helper", 11),
            ("run", "local", 12),
            ("run", "helper", 13),
            ("run", "listOf", 13),
            ("run", "map", 13),
        ]
    );
}

#[test]
fn a_call_to_a_local_lambda_resolves() {
    let root = std::env::temp_dir().join(format!("mct-kt-lambdas-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/A.kt"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(KotlinParser));
    let hits: Vec<String> = index
        .find_references("local")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["src/A.kt:12 Resolved"]);
}
