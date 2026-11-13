//! A lambda bound to one name (`double = lambda x: x * 2`) is a `function`
//! of the module, or a `method` of the class whose body binds it, as a JS
//! arrow bound to a `const` is: the name is how the code calls it. Its
//! signature is the binding as written up to the body, a method's first
//! parameter is its receiver, and a call in its body is a call from it. A
//! lambda among several targets (`a, b = lambda: 1, lambda: 2`) or inside a
//! function declares nothing new.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParameterKind, ParsedFile, RelationKind, SymbolKind};
use mct_lang_python::PythonParser;

const SOURCE: &str = r#"double = lambda x: x * 2
pair = lambda a, b=1: helper(a)
class Box:
    size = lambda self: len(self.items)
first, second = lambda: 1, lambda: 2
def outer():
    inner = lambda: helper(0)
    return inner
def helper(v):
    return v
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&PythonParser, "src/shapes.py", "Python", SOURCE)
}

fn signature(parsed: &ParsedFile, name: &str) -> Option<String> {
    let symbol = parsed.symbols.iter().find(|s| s.name == name)?;
    parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == symbol.id)
        .and_then(|d| d.signature.clone())
}

#[test]
fn a_lambda_bound_to_one_name_is_a_function_or_a_method() {
    let parsed = parsed();
    let mut rows: Vec<(String, SymbolKind, Option<String>, u32)> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind != SymbolKind::Module)
        .map(|s| (s.name.clone(), s.kind, s.parent.clone(), s.location.line))
        .collect();
    rows.sort_by_key(|r| (r.3, r.0.clone()));
    let row = |name: &str, kind: SymbolKind, parent: Option<&str>, line: u32| {
        (name.to_string(), kind, parent.map(str::to_string), line)
    };
    assert_eq!(
        rows,
        [
            row("double", SymbolKind::Function, None, 1),
            row("pair", SymbolKind::Function, None, 2),
            row("Box", SymbolKind::Class, None, 3),
            row("size", SymbolKind::Method, Some("Box"), 4),
            row("first", SymbolKind::Variable, None, 5),
            row("second", SymbolKind::Variable, None, 5),
            row("outer", SymbolKind::Function, None, 6),
            row("helper", SymbolKind::Function, None, 9),
        ]
    );
    assert_eq!(
        signature(&parsed, "double").as_deref(),
        Some("double = lambda x")
    );
    assert_eq!(
        signature(&parsed, "pair").as_deref(),
        Some("pair = lambda a, b=1")
    );
    assert_eq!(
        signature(&parsed, "size").as_deref(),
        Some("size = lambda self")
    );
}

#[test]
fn a_method_lambda_binds_its_receiver() {
    let parsed = parsed();
    let size = parsed.symbols.iter().find(|s| s.name == "size").unwrap();
    let detail = parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == size.id)
        .unwrap();
    let kinds: Vec<ParameterKind> = detail.parameters.iter().map(|p| p.kind).collect();
    assert_eq!(kinds, [ParameterKind::Receiver]);
}

#[test]
fn a_call_in_the_body_is_a_call_from_the_lambda() {
    let parsed = parsed();
    let name_of = |id| {
        parsed
            .symbols
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.as_str())
            .unwrap_or("?")
    };
    let mut calls: Vec<(&str, &str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| (name_of(r.from), r.to_name.as_str(), r.location.line))
        .collect();
    calls.sort_by_key(|c| c.2);
    assert_eq!(
        calls,
        [
            ("pair", "helper", 2),
            ("size", "len", 4),
            ("outer", "helper", 7)
        ]
    );
}
