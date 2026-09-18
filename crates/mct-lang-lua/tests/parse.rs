// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-lua/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_lua::LuaParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    LuaParser
        .parse(&SourceFile {
            relative_path: "module.lua".to_string(),
            contents: src.to_string(),
        })
        .expect("valid Lua source should parse")
}

#[test]
fn extracts_named_function_and_call() {
    let parsed = parse("function add(a, b)\n  return helper(a, b)\nend\n");
    let func = parsed
        .symbols
        .iter()
        .find(|s| s.name == "add")
        .expect("add should be indexed");
    assert_eq!(func.kind, SymbolKind::Function);

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"helper"));
}

#[test]
fn extracts_anonymous_function_assigned_to_local() {
    let parsed = parse("local square = function(x)\n  return x * x\nend\n");
    let func = parsed
        .symbols
        .iter()
        .find(|s| s.name == "square")
        .expect("square (anonymous function bound to a local) should be indexed");
    assert_eq!(func.kind, SymbolKind::Function);
}

#[test]
fn extracts_table_module_with_dotted_and_method_functions() {
    let parsed = parse(
        "local M = {}\n\nfunction M.new(x)\n  return x\nend\n\nfunction M:greet()\n  print('hi')\nend\n\nreturn M\n",
    );
    let module = parsed
        .symbols
        .iter()
        .find(|s| s.name == "M")
        .expect("M table should be indexed as a module");
    assert_eq!(module.kind, SymbolKind::Module);

    let new_fn = parsed
        .symbols
        .iter()
        .find(|s| s.name == "new")
        .expect("M.new should be indexed");
    assert_eq!(new_fn.kind, SymbolKind::Method);
    assert_eq!(new_fn.parent.as_deref(), Some("M"));

    let greet_fn = parsed
        .symbols
        .iter()
        .find(|s| s.name == "greet")
        .expect("M:greet should be indexed");
    assert_eq!(greet_fn.kind, SymbolKind::Method);
    assert_eq!(greet_fn.parent.as_deref(), Some("M"));

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"print"));
}

#[test]
fn extracts_require_as_import() {
    let parsed = parse("local other = require('other_module')\n");
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"other_module"));
}

#[test]
fn extracts_nested_dotted_function_declaration() {
    let parsed = parse("function Foo.Bar.baz()\nend\n");
    let baz = parsed
        .symbols
        .iter()
        .find(|s| s.name == "baz")
        .expect("Foo.Bar.baz should be indexed");
    assert_eq!(baz.parent.as_deref(), Some("Foo.Bar"));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = LuaParser.parse(&SourceFile {
        relative_path: "broken.lua".to_string(),
        contents: "function broken(\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}
