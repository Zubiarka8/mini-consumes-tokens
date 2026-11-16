//! A `function` declared inside a function or method body, or a function
//! value bound there (`const show = () => …`), is a `function` owned by it, as a Rust or Python nested function is; it never becomes a
//! method. A top-level function and one in a class static block have no
//! parent. A call from the outer function to its nested helper still
//! reaches it, even when another file has a top-level function of that name.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, SymbolKind};
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"export function outer() {
  function helper() {
    function* deepest() {}
    return deepest();
  }
  return helper();
}
export function Table() {
  const show = (n: number) => n;
  return show(1);
}
export class Shop {
  total() {
    function add(a, b) { return a + b; }
    return add(1, 2);
  }
  static { function boot() {} boot(); }
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JsTsParser, "src/shop.ts", "TypeScript", SOURCE)
}

#[test]
fn a_nested_function_belongs_to_its_enclosing_function() {
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
            ("Table", SymbolKind::Function, None),
            ("show", SymbolKind::Function, Some("Table")),
            ("Shop", SymbolKind::Class, None),
            ("total", SymbolKind::Method, Some("Shop")),
            ("add", SymbolKind::Function, Some("total")),
            ("boot", SymbolKind::Function, None),
        ]
    );
}

#[test]
fn a_call_to_a_nested_helper_still_resolves() {
    let root = std::env::temp_dir().join(format!("mct-ts-nested-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/shop.ts"), SOURCE).unwrap();
    // Another file's top-level functions of the same names: the nested ones
    // are still this file's, so the calls stay with them.
    std::fs::write(
        root.join("src/other.ts"),
        "export function helper() {}\nexport function show() {}\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    let mut hits: Vec<String> = ["helper", "deepest", "show", "add", "boot"]
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
            "src/shop.ts:10 show Resolved",
            "src/shop.ts:15 add Resolved",
            "src/shop.ts:17 boot Resolved",
            "src/shop.ts:4 deepest Resolved",
            "src/shop.ts:6 helper Resolved",
        ]
    );
}
