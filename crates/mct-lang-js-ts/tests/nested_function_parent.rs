//! A function declared inside another function's body is a nested
//! function: its `parent` is the enclosing function. A top-level function,
//! a method of a class declared inside a function, and an arrow function
//! assigned to a name keep their own parent rules.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, SymbolKind};
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"function outer() {
  function inner() {
    function deepest() {}
  }
  class Local {
    run() {
      function insideMethod() {}
    }
  }
  return inner;
}
function sibling() {}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JsTsParser, "src/nest.ts", "TypeScript", SOURCE)
}

fn parent_of(file: &ParsedFile, name: &str, kind: SymbolKind) -> Option<String> {
    file.symbols
        .iter()
        .find(|s| s.name == name && s.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?} `{name}`"))
        .parent
        .clone()
}

#[test]
fn a_function_declared_in_a_function_has_it_as_parent() {
    let file = parsed();
    assert_eq!(parent_of(&file, "outer", SymbolKind::Function), None);
    assert_eq!(parent_of(&file, "sibling", SymbolKind::Function), None);
    assert_eq!(
        parent_of(&file, "inner", SymbolKind::Function).as_deref(),
        Some("outer")
    );
    assert_eq!(
        parent_of(&file, "deepest", SymbolKind::Function).as_deref(),
        Some("inner")
    );
}

#[test]
fn a_method_is_the_enclosing_function_of_its_own_nested_function() {
    let file = parsed();
    assert_eq!(parent_of(&file, "Local", SymbolKind::Class), None);
    assert_eq!(
        parent_of(&file, "run", SymbolKind::Method).as_deref(),
        Some("Local")
    );
    assert_eq!(
        parent_of(&file, "insideMethod", SymbolKind::Function).as_deref(),
        Some("run")
    );
}

#[test]
fn an_arrow_assigned_to_a_name_in_a_function_has_it_as_parent() {
    let file = mct_corpus::parse_source(
        &JsTsParser,
        "src/arrow.ts",
        "TypeScript",
        "function outer() {\n  const helper = () => 1;\n  return helper();\n}\nconst top = () => 2;\n",
    );
    assert_eq!(
        parent_of(&file, "helper", SymbolKind::Function).as_deref(),
        Some("outer")
    );
    assert_eq!(parent_of(&file, "top", SymbolKind::Function), None);
}
