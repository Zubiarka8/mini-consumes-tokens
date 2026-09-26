// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-cpp/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_cpp::CppParser;

fn parse(relative_path: &str, src: &str) -> mct_core::ParsedFile {
    CppParser
        .parse(&SourceFile {
            relative_path: relative_path.to_string(),
            contents: src.to_string(),
        })
        .expect("valid C++ source should parse")
}

#[test]
fn extracts_function_and_call() {
    let parsed = parse(
        "calc.cpp",
        "int helper(int a, int b) {\n    return a + b;\n}\n\nint add(int a, int b) {\n    return helper(a, b);\n}\n",
    );
    let add = parsed.symbols.iter().find(|s| s.name == "add").unwrap();
    assert_eq!(add.kind, SymbolKind::Function);
    assert_eq!(add.parent, None);

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"helper"));
}

#[test]
fn header_declaration_and_source_definition_share_name_and_parent() {
    let header = parse(
        "Invoice.h",
        "class Invoice {\npublic:\n    double addItem(double price);\n};\n",
    );
    let decl = header.symbols.iter().find(|s| s.name == "addItem").unwrap();
    assert_eq!(decl.kind, SymbolKind::Method);
    assert_eq!(decl.parent.as_deref(), Some("Invoice"));

    let source = parse(
        "Invoice.cpp",
        "double Invoice::addItem(double price) {\n    return price;\n}\n",
    );
    let def = source.symbols.iter().find(|s| s.name == "addItem").unwrap();
    assert_eq!(def.kind, SymbolKind::Method);
    // Same name + same parent as the header declaration above — the two
    // rows a real multi-file index would produce correlate as the same
    // logical member, without needing to touch `mct-core`/`mct-index` to
    // add a cross-file symbol-identity concept. See lib.rs's module doc.
    assert_eq!(def.parent.as_deref(), Some("Invoice"));
}

#[test]
fn template_function_is_indexed_without_resolving_instantiations() {
    let parsed = parse(
        "util.h",
        "template <typename T>\nT maxValue(T a, T b) {\n    return a > b ? a : b;\n}\n",
    );
    // AST-only scope: the generic signature is indexed as a normal
    // Function symbol; no attempt is made to resolve `maxValue<int>` vs
    // `maxValue<double>` as distinct instantiations.
    let max_value = parsed
        .symbols
        .iter()
        .find(|s| s.name == "maxValue")
        .unwrap();
    assert_eq!(max_value.kind, SymbolKind::Function);
}

#[test]
fn operator_overload_is_a_normal_method_not_a_special_case() {
    let parsed = parse(
        "complex.h",
        "class Complex {\npublic:\n    Complex operator+(const Complex& other) const {\n        return Complex();\n    }\n};\n",
    );
    let op = parsed
        .symbols
        .iter()
        .find(|s| s.name == "operator+")
        .unwrap();
    assert_eq!(op.kind, SymbolKind::Method);
    assert_eq!(op.parent.as_deref(), Some("Complex"));
}

#[test]
fn extracts_inheritance_and_distinguishes_local_from_system_includes() {
    let parsed = parse(
        "shapes.h",
        "#include \"Shape.h\"\n#include <vector>\n\nclass Circle : public Shape {\npublic:\n    double area() { return 0.0; }\n};\n",
    );
    let extends: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Extends)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(extends.contains(&"Shape"));

    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    // Local include keeps its repo-relative name; system include loses the
    // angle brackets but is never expected to resolve to a file in this
    // repo's index (see `Walker::visit`'s `preproc_include` arm).
    assert!(imports.contains(&"Shape.h"));
    assert!(imports.contains(&"vector"));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = CppParser.parse(&SourceFile {
        relative_path: "broken.cpp".to_string(),
        contents: "class Broken {\n  void go( {\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}
