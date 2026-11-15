//! A package-level `var f = func(…) {…}` (one name, no type) is a
//! `function` of the package, as a JS arrow or a Python lambda bound to a
//! name is: the name is how the code calls it. Its signature is the binding
//! up to the body, and a call in the body is a call from it. Any other
//! value, several names, a declared type or a `:=` inside a function stays
//! as before.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_go::GoParser;

const SOURCE: &str = r#"package shop

var now = func() int64 { return clock() }

var (
	format = func(v int) string { return render(v) }
	limit  = 10
)

var a, b = func() {}, func() {}

var typed func() = func() {}

func clock() int64 { return 0 }

func render(v int) string {
	local := func() { clock() }
	local()
	return ""
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&GoParser, "shop/values.go", "Go", SOURCE)
}

#[test]
fn a_package_func_value_is_a_function() {
    let parsed = parsed();
    let rows: Vec<(SymbolKind, &str)> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind != SymbolKind::Module)
        .map(|s| (s.kind, s.name.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            (SymbolKind::Function, "now"),
            (SymbolKind::Function, "format"),
            (SymbolKind::Variable, "limit"),
            (SymbolKind::Variable, "a"),
            (SymbolKind::Variable, "b"),
            (SymbolKind::Variable, "typed"),
            (SymbolKind::Function, "clock"),
            (SymbolKind::Function, "render"),
        ]
    );
    let signature = |name: &str| {
        let id = parsed.symbols.iter().find(|s| s.name == name).unwrap().id;
        parsed
            .symbol_details
            .iter()
            .find(|d| d.symbol == id)
            .and_then(|d| d.signature.clone())
    };
    assert_eq!(signature("now").as_deref(), Some("var now = func() int64"));
    assert_eq!(
        signature("format").as_deref(),
        Some("var format = func(v int) string")
    );
}

#[test]
fn a_call_in_the_body_is_a_call_from_the_function() {
    let parsed = parsed();
    let name_of = |id| {
        parsed
            .symbols
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.as_str())
            .unwrap()
    };
    let calls: Vec<(&str, &str)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| (name_of(r.from), r.to_name.as_str()))
        .collect();
    assert_eq!(
        calls,
        [
            ("now", "clock"),
            ("format", "render"),
            ("render", "clock"),
            ("render", "local"),
        ]
    );
}
