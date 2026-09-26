// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-js-ts/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_js_ts::JsTsParser;

fn parse(relative_path: &str, src: &str) -> mct_core::ParsedFile {
    JsTsParser
        .parse(&SourceFile {
            relative_path: relative_path.to_string(),
            contents: src.to_string(),
        })
        .expect("valid JS/TS source should parse")
}

#[test]
fn arrow_function_assigned_to_variable_is_a_function_symbol() {
    let parsed = parse(
        "math.js",
        "const add = (a, b) => {\n    return helper(a, b);\n};\n\nfunction helper(a, b) {\n    return a + b;\n}\n",
    );
    let add = parsed.symbols.iter().find(|s| s.name == "add").unwrap();
    assert_eq!(add.kind, SymbolKind::Function);

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(
        calls.contains(&"helper"),
        "call inside the arrow function body should be attributed to it: {calls:?}"
    );
}

#[test]
fn es_module_import_and_named_export_are_recorded() {
    let parsed = parse(
        "utils.ts",
        "import { log } from \"./logger\";\n\nexport function greet(name: string): void {\n    log(name);\n}\n\nexport { greet as sayHello };\n",
    );
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"log"));

    let greet = parsed.symbols.iter().find(|s| s.name == "greet").unwrap();
    assert_eq!(greet.kind, SymbolKind::Function);

    let references: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(
        references.contains(&"greet"),
        "`export {{ greet as sayHello }}` should reference the local name: {references:?}"
    );
}

#[test]
fn import_alias_uses_local_bound_name() {
    let parsed = parse(
        "consumer.js",
        "import { foo as bar } from \"./mod\";\n\nbar();\n",
    );
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(
        imports.contains(&"bar"),
        "aliased import should record the local alias, not the source name: {imports:?}"
    );
    assert!(!imports.contains(&"foo"));
}

#[test]
fn commonjs_require_and_module_exports_are_recorded() {
    let parsed = parse(
        "mathUtils.js",
        "function add(a, b) {\n    return a + b;\n}\n\nmodule.exports = { add };\n",
    );
    let add = parsed
        .symbols
        .iter()
        .find(|s| s.name == "add" && s.kind == SymbolKind::Function)
        .unwrap();
    assert_eq!(add.kind, SymbolKind::Function);

    let references: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(
        references.contains(&"add"),
        "module.exports shorthand should reference the exported name: {references:?}"
    );
}

#[test]
fn require_call_is_an_import_relation() {
    let parsed = parse(
        "consumer.js",
        "const { add } = require(\"./mathUtils\");\n\nadd(1, 2);\n",
    );
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(
        imports.contains(&"./mathUtils"),
        "require() should record the module path: {imports:?}"
    );
}

#[test]
fn class_implements_interface_and_extends_base() {
    let parsed = parse(
        "shapes.ts",
        "interface Shape {\n    area(): number;\n}\n\nclass BaseShape {}\n\nclass Circle extends BaseShape implements Shape {\n    area(): number {\n        return 0;\n    }\n}\n",
    );
    let shape = parsed.symbols.iter().find(|s| s.name == "Shape").unwrap();
    assert_eq!(shape.kind, SymbolKind::Interface);

    let extends: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Extends)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(extends.contains(&"BaseShape"));

    let implements: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Implements)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(implements.contains(&"Shape"));

    let area = parsed.symbols.iter().find(|s| s.name == "area").unwrap();
    assert_eq!(area.kind, SymbolKind::Method);
    assert_eq!(area.parent.as_deref(), Some("Circle"));
}

#[test]
fn type_alias_is_extracted() {
    let parsed = parse("types.ts", "export type Id = string | number;\n");
    let id_type = parsed.symbols.iter().find(|s| s.name == "Id").unwrap();
    assert_eq!(id_type.kind, SymbolKind::TypeAlias);
}

#[test]
fn tsx_component_logic_is_indexed_without_structuring_jsx() {
    let parsed = parse(
        "Widget.tsx",
        "function useCount(): number {\n    return 1;\n}\n\nexport function Widget() {\n    const count = useCount();\n    return (\n        <div>\n            <button onClick={() => useCount()}>{count}</button>\n        </div>\n    );\n}\n",
    );
    // "Widget" is ambiguous by name alone: the file-level module
    // pseudo-symbol (named after Widget.tsx) collides with the exported
    // component of the same name — disambiguate by kind, same as the Java
    // plugin's `Calculator`/`Calculator.java` test.
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == "Widget" && s.kind == SymbolKind::Function));

    // No JSX-specific SymbolKind/RelationKind exists (or is expected) — only
    // the logic embedded in the component (the hook call from the render
    // body, and the one inside the onClick handler) should show up as Calls.
    let use_count_calls = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls && r.to_name == "useCount")
        .count();
    assert_eq!(
        use_count_calls, 2,
        "one call in the component body, one inside the JSX event handler"
    );
}

#[test]
fn function_end_line_spans_the_whole_multiline_body() {
    let parsed = parse(
        "multi.js",
        "function helper(a, b) {\n    const sum = a + b;\n    return sum;\n}\n",
    );
    let sym = parsed
        .symbols
        .iter()
        .find(|s| s.name == "helper")
        .expect("helper function should be indexed");
    assert_eq!(sym.location.line, 1);
    assert_eq!(sym.location.end_line, Some(4));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = JsTsParser.parse(&SourceFile {
        relative_path: "Broken.ts".to_string(),
        contents: "function broken( {\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}
