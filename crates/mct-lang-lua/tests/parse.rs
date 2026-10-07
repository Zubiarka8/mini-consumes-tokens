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
fn file_module_ends_on_the_last_line() {
    // A trailing newline must not push the module one line past the file.
    let module = |src: &str| {
        let parsed = parse(src);
        let m = parsed
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module && s.name == "module")
            .unwrap();
        (m.location.line, m.location.end_line)
    };
    let body = "local function f()\n  return 1\nend";
    assert_eq!(module(&format!("{body}\n")), (1, Some(3)));
    assert_eq!(module(body), (1, Some(3)));
    assert_eq!(module("f()\n"), (1, Some(1)));
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

fn symbol<'a>(parsed: &'a mct_core::ParsedFile, name: &str) -> &'a mct_core::SymbolRecord {
    parsed
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol {name}"))
}

#[test]
fn a_function_assigned_to_a_target_spans_its_body() {
    let parsed = parse("local Money = {}\nMoney.__add = function(a, b)\n  return a:plus(b)\nend\n");
    let add = symbol(&parsed, "__add");
    assert_eq!((add.location.line, add.location.end_line), (2, Some(4)));
    // `T.f = function` is a method of T, like `function T.f()`.
    assert_eq!(
        (add.kind, add.parent.as_deref()),
        (SymbolKind::Method, Some("Money"))
    );
}

#[test]
fn only_top_level_named_tables_are_modules() {
    let parsed = parse(
        "local M = {}\nM.cache = {}\nfunction M.route(self, p)\n  local out = { p }\n  self.routes[#self.routes + 1] = { p }\n  return out\nend\nreturn M\n",
    );
    let modules: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Module)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(modules, ["module", "M", "cache"]);
}

#[test]
fn functions_in_a_table_constructor_are_methods_of_the_table() {
    let parsed = parse(
        "local handlers = {\n  get_stock = function(p)\n    return check(p)\n  end,\n  limit = 10,\n}\n",
    );
    let f = symbol(&parsed, "get_stock");
    assert_eq!(
        (
            f.kind,
            f.parent.as_deref(),
            f.location.line,
            f.location.end_line
        ),
        (SymbolKind::Method, Some("handlers"), 2, Some(4))
    );
    let call = parsed
        .relations
        .iter()
        .find(|r| r.to_name == "check")
        .unwrap();
    assert_eq!(call.from, f.id);
}

#[test]
fn calls_in_the_called_expression_are_extracted() {
    let parsed = parse("local function show(net)\n  return money.gross(net, 21):to_json()\nend\n");
    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert_eq!(calls, ["to_json", "gross"]);
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = LuaParser.parse(&SourceFile {
        relative_path: "broken.lua".to_string(),
        contents: "function broken(\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}
