//! A local bound to a lambda or an anonymous method (`Func<int, int> twice
//! = x => …;`, `Action log = delegate { … };`) is a `function`, as a local
//! function is: it carries its declaration and parameters, owns the calls
//! in its body, and a call through the local (`twice(2)`) reaches it. A
//! field holding a lambda stays a field, a lambda passed as an argument
//! declares nothing, and another declarator of the same statement stays a
//! plain local.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"class S {
  Func<int> cb = () => Helper();
  static int Helper() => 1;
  void Run() {
    Func<int, int> twice = x => Helper() * x, plain = null;
    var add = async (int a, int b) => await Sum(a, b);
    Action log = delegate(int q) { Helper(); };
    int Local() => Helper();
    twice(2);
    list.Select(x => Helper());
  }
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CSharpParser, "src/S.cs", "C#", SOURCE)
}

fn name_of(parsed: &ParsedFile, id: mct_core::SymbolId) -> String {
    let s = parsed.symbols.iter().find(|s| s.id == id).unwrap();
    format!("{} {:?}", s.name, s.kind)
}

#[test]
fn a_local_bound_to_a_lambda_is_a_function() {
    let parsed = parsed();
    let symbols: Vec<(&str, SymbolKind, Option<&str>, u32)> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind != SymbolKind::Module)
        .map(|s| {
            (
                s.name.as_str(),
                s.kind,
                s.parent.as_deref(),
                s.location.line,
            )
        })
        .collect();
    assert_eq!(
        symbols,
        [
            ("S", SymbolKind::Class, None, 1),
            ("cb", SymbolKind::Field, Some("S"), 2),
            ("Helper", SymbolKind::Method, Some("S"), 3),
            ("Run", SymbolKind::Method, Some("S"), 4),
            // A local function has no parent; a named lambda neither.
            ("twice", SymbolKind::Function, None, 5),
            ("add", SymbolKind::Function, None, 6),
            ("log", SymbolKind::Function, None, 7),
            ("Local", SymbolKind::Function, None, 8),
        ]
    );
}

#[test]
fn a_named_lambda_carries_its_declaration() {
    let parsed = parsed();
    let detail = |name: &str| {
        let id = parsed.symbols.iter().find(|s| s.name == name).unwrap().id;
        parsed
            .symbol_details
            .iter()
            .find(|d| d.symbol == id)
            .unwrap()
            .clone()
    };
    let params = |name: &str| -> Vec<(Option<String>, Option<String>)> {
        detail(name)
            .parameters
            .iter()
            .map(|p| (p.name.clone(), p.type_text.as_ref().map(|t| t.text.clone())))
            .collect()
    };
    assert_eq!(
        detail("twice").signature.as_deref(),
        Some("Func<int, int> twice = x =>")
    );
    assert_eq!(params("twice"), [(Some("x".to_string()), None)]);
    assert_eq!(
        detail("add").signature.as_deref(),
        Some("var add = async (int a, int b) =>")
    );
    assert_eq!(
        params("add"),
        [
            (Some("a".to_string()), Some("int".to_string())),
            (Some("b".to_string()), Some("int".to_string())),
        ]
    );
    assert_eq!(detail("add").modifiers, [mct_core::Modifier::Async]);
    assert_eq!(
        detail("log").signature.as_deref(),
        Some("Action log = delegate(int q)")
    );
    assert_eq!(
        params("log"),
        [(Some("q".to_string()), Some("int".to_string()))]
    );
}

#[test]
fn a_named_lambda_owns_the_calls_in_its_body() {
    let parsed = parsed();
    let mut calls: Vec<(String, &str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| {
            (
                name_of(&parsed, r.from),
                r.to_name.as_str(),
                r.location.line,
            )
        })
        .collect();
    calls.sort();
    assert_eq!(
        calls,
        [
            ("Local Function".to_string(), "Helper", 8),
            ("Run Method".to_string(), "Helper", 10),
            ("Run Method".to_string(), "Select", 10),
            ("Run Method".to_string(), "twice", 9),
            ("add Function".to_string(), "Sum", 6),
            ("cb Field".to_string(), "Helper", 2),
            ("log Function".to_string(), "Helper", 7),
            ("twice Function".to_string(), "Helper", 5),
        ]
    );
}

#[test]
fn a_call_through_the_local_reaches_the_lambda() {
    let root = std::env::temp_dir().join(format!("mct-cs-named-lambdas-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/S.cs"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CSharpParser));
    let hits: Vec<String> = index
        .find_references("twice")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["src/S.cs:9 Resolved"]);
}
