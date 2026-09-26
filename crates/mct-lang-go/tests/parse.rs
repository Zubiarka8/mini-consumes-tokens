// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-go/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_go::GoParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    GoParser
        .parse(&SourceFile {
            relative_path: "calc.go".to_string(),
            contents: src.to_string(),
        })
        .expect("valid Go source should parse")
}

#[test]
fn extracts_function_and_call() {
    let parsed = parse(
        "package calc\n\nfunc helper(a int, b int) int {\n\treturn a + b\n}\n\nfunc add(a int, b int) int {\n\treturn helper(a, b)\n}\n",
    );
    let add = parsed.symbols.iter().find(|s| s.name == "add").unwrap();
    assert_eq!(add.kind, SymbolKind::Function);
    assert_eq!(
        add.parent.as_deref(),
        Some("calc"),
        "top-level function's parent is its package"
    );

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"helper"));
}

#[test]
fn method_with_pointer_receiver_attaches_to_its_type() {
    let parsed = parse(
        "package billing\n\ntype Invoice struct {\n\tTotal float64\n}\n\nfunc (inv *Invoice) AddItem(price float64) float64 {\n\tinv.Total += price\n\treturn inv.Total\n}\n",
    );
    let invoice = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Invoice" && s.kind == SymbolKind::Struct)
        .unwrap();
    assert_eq!(invoice.parent.as_deref(), Some("billing"));

    let add_item = parsed.symbols.iter().find(|s| s.name == "AddItem").unwrap();
    assert_eq!(add_item.kind, SymbolKind::Method);
    assert_eq!(
        add_item.parent.as_deref(),
        Some("Invoice"),
        "pointer receiver `*Invoice` must unwrap to the plain type name"
    );
}

#[test]
fn method_with_value_receiver_also_attaches_to_its_type() {
    let parsed = parse(
        "package billing\n\ntype Counter struct {\n\tValue int\n}\n\nfunc (c Counter) Get() int {\n\treturn c.Value\n}\n",
    );
    let get = parsed.symbols.iter().find(|s| s.name == "Get").unwrap();
    assert_eq!(get.kind, SymbolKind::Method);
    assert_eq!(get.parent.as_deref(), Some("Counter"));
}

#[test]
fn interface_and_its_declared_methods_are_indexed_without_resolving_implementers() {
    let parsed = parse(
        "package billing\n\ntype Shape interface {\n\tArea() float64\n\tPerimeter() float64\n}\n",
    );
    let shape = parsed.symbols.iter().find(|s| s.name == "Shape").unwrap();
    assert_eq!(shape.kind, SymbolKind::Interface);

    let area = parsed.symbols.iter().find(|s| s.name == "Area").unwrap();
    assert_eq!(area.kind, SymbolKind::Method);
    assert_eq!(area.parent.as_deref(), Some("Shape"));

    let perimeter = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Perimeter")
        .unwrap();
    assert_eq!(perimeter.parent.as_deref(), Some("Shape"));

    // No Implements relation is ever produced by this parser — Go interface
    // satisfaction is structural, not declared, and resolving it needs a
    // type checker (deferred; see this crate's module doc).
    assert!(parsed
        .relations
        .iter()
        .all(|r| r.kind != RelationKind::Implements));
}

#[test]
fn extracts_package_and_import() {
    let parsed =
        parse("package billing\n\nimport \"fmt\"\n\nfunc noop() {\n\tfmt.Println(\"hi\")\n}\n");
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == "billing" && s.kind == SymbolKind::Module));

    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"fmt"));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = GoParser.parse(&SourceFile {
        relative_path: "broken.go".to_string(),
        contents: "package broken\n\nfunc go( {\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}
