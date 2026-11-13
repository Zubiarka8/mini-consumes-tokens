//! A `def` inside a function is a `function` owned by the enclosing
//! function or method, as a Rust or Go nested function is; a `def` directly
//! in a class body stays a `method` of the class. A call from the outer
//! function to its nested helper still reaches it.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, SymbolKind};
use mct_lang_python::PythonParser;

const SOURCE: &str = r#"def outer():
    def helper():
        def deepest():
            pass
        return deepest()
    return helper()

class Shop:
    def total(self):
        def add(a, b):
            return a + b
        return add(1, 2)

    class Inner:
        def run(self):
            pass
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&PythonParser, "shop.py", "Python", SOURCE)
}

#[test]
fn a_nested_def_belongs_to_its_enclosing_function() {
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
            ("outer", SymbolKind::Function, None),
            ("helper", SymbolKind::Function, Some("outer")),
            ("deepest", SymbolKind::Function, Some("helper")),
            ("Shop", SymbolKind::Class, None),
            ("total", SymbolKind::Method, Some("Shop")),
            ("add", SymbolKind::Function, Some("total")),
            ("Inner", SymbolKind::Class, Some("Shop")),
            ("run", SymbolKind::Method, Some("Inner")),
        ]
    );
}

#[test]
fn a_call_to_a_nested_helper_still_resolves() {
    let root = std::env::temp_dir().join(format!("mct-py-nested-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("shop.py"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PythonParser));
    let mut hits: Vec<String> = ["helper", "deepest", "add"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| {
            format!(
                "{}:{} {} {:?}",
                h.relative_path, h.line, h.to_name, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "shop.py:12 add Resolved",
            "shop.py:5 deepest Resolved",
            "shop.py:6 helper Resolved",
        ]
    );
}
