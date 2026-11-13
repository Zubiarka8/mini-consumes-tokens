//! A lambda bound to a name (`auto twice = [](int x) { … };`) is a
//! `function` with the binding up to the body as signature and the lambda's
//! parameters, as a Rust `let f = |x| …` or a Python `f = lambda …` is. At
//! namespace scope it belongs to the namespace; in a function body it is
//! owned by that function, so a call to it resolves. The calls in its body
//! are its own. A data member holding a lambda (`std::function<void()> cb =
//! [] { … };`) stays a `field`, and a lambda that is no initializer of a
//! plain name declares nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_cpp::CppParser;

const SOURCE: &str = r#"int helper();
auto twice = [](int x) { return helper() * x; };
namespace ns {
const auto add = [](int a, int b) { return a + b; };
}
struct S {
  std::function<void()> cb = [] { helper(); };
};
void run(std::vector<int>& v) {
  auto local = [] { helper(); };
  local();
  std::sort(v.begin(), v.end(), [](int a, int b) { return a < b; });
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CppParser, "src/a.cpp", "C++", SOURCE)
}

#[test]
fn a_lambda_bound_to_a_name_is_a_function() {
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
            ("helper", SymbolKind::Function, None),
            ("twice", SymbolKind::Function, None),
            ("add", SymbolKind::Function, Some("ns")),
            ("S", SymbolKind::Struct, None),
            ("cb", SymbolKind::Field, Some("S")),
            ("run", SymbolKind::Function, None),
            ("local", SymbolKind::Function, Some("run")),
        ]
    );
    let twice = parsed.symbols.iter().find(|s| s.name == "twice").unwrap();
    let detail = parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == twice.id)
        .unwrap();
    assert_eq!(detail.signature.as_deref(), Some("auto twice = [](int x)"));
    let params: Vec<Option<&str>> = detail
        .parameters
        .iter()
        .map(|p| p.name.as_deref())
        .collect();
    assert_eq!(params, [Some("x")]);
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
    let calls: Vec<(&str, &str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| (name_of(r.from), r.to_name.as_str(), r.location.line))
        .collect();
    assert_eq!(
        calls,
        [
            ("twice", "helper", 2),
            ("a", "helper", 7),
            ("local", "helper", 10),
            ("run", "local", 11),
            ("run", "sort", 12),
            ("run", "begin", 12),
            ("run", "end", 12),
        ]
    );
}

#[test]
fn a_call_to_a_local_lambda_resolves() {
    let root = std::env::temp_dir().join(format!("mct-cpp-lambdas-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.cpp"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CppParser));
    let hits: Vec<String> = index
        .find_references("local")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["src/a.cpp:11 Resolved"]);
}
