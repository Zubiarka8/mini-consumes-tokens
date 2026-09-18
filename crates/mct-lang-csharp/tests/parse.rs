// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-csharp/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_csharp::CSharpParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    CSharpParser
        .parse(&SourceFile {
            relative_path: "Calculator.cs".to_string(),
            contents: src.to_string(),
        })
        .expect("valid C# source should parse")
}

#[test]
fn extracts_namespace_class_method_and_call() {
    let parsed = parse(
        "namespace App {\n    public class Calculator {\n        public int Add(int a, int b) {\n            return Helper(a, b);\n        }\n\n        private int Helper(int a, int b) {\n            return a + b;\n        }\n    }\n}\n",
    );
    let ns = parsed.symbols.iter().find(|s| s.name == "App").unwrap();
    assert_eq!(ns.kind, SymbolKind::Module);

    // "Calculator" is ambiguous by name alone: the file-level module
    // pseudo-symbol (named after Calculator.cs) collides with the class of
    // the same name — the idiomatic Java/C# convention of naming a file
    // after its public type. Both legitimately exist in the index; a real
    // query disambiguates by kind, same as here.
    let class = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Calculator" && s.kind == SymbolKind::Class)
        .unwrap();
    assert_eq!(class.parent.as_deref(), Some("App"));

    let helper = parsed.symbols.iter().find(|s| s.name == "Helper").unwrap();
    assert_eq!(helper.parent.as_deref(), Some("Calculator"));

    let calls: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Calls).map(|r| r.to_name.as_str()).collect();
    assert!(calls.contains(&"Helper"));
}

#[test]
fn overloaded_methods_are_kept_as_separate_symbols() {
    let parsed = parse(
        "class Calculator {\n    public int Add(int a, int b) {\n        return a + b;\n    }\n\n    public double Add(double a, double b) {\n        return a + b;\n    }\n}\n",
    );
    let adds: Vec<_> = parsed.symbols.iter().filter(|s| s.name == "Add").collect();
    assert_eq!(adds.len(), 2, "both overloads of Add() should be indexed as distinct symbols");
    assert_ne!(adds[0].location.line, adds[1].location.line);
}

#[test]
fn property_with_accessors_is_one_symbol_not_two() {
    let parsed = parse(
        "class Person {\n    public string Name { get; set; }\n\n    public int Age {\n        get { return age; }\n        set { age = value; }\n    }\n}\n",
    );
    let name_props: Vec<_> = parsed.symbols.iter().filter(|s| s.name == "Name").collect();
    assert_eq!(name_props.len(), 1, "auto-property must be a single symbol, not get+set");
    assert_eq!(name_props[0].kind, SymbolKind::Field);

    let age_props: Vec<_> = parsed.symbols.iter().filter(|s| s.name == "Age").collect();
    assert_eq!(age_props.len(), 1, "property with custom get/set must still be a single symbol");

    // No separate "get"/"set" symbols should have been created.
    assert!(parsed.symbols.iter().all(|s| s.name != "get" && s.name != "set"));

    // Calls inside the custom accessor bodies attach to the property symbol.
    let calls: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Calls).collect();
    assert!(calls.is_empty(), "this fixture's accessors have no calls, sanity check");
}

#[test]
fn base_class_and_interfaces_are_distinguished_by_position() {
    let parsed = parse(
        "public interface IShape {\n    double Area();\n}\n\npublic class Circle : BaseShape, IShape {\n    public double Area() {\n        return 0.0;\n    }\n}\n",
    );
    let ishape = parsed.symbols.iter().find(|s| s.name == "IShape").unwrap();
    assert_eq!(ishape.kind, SymbolKind::Interface);

    let extends: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Extends).map(|r| r.to_name.as_str()).collect();
    assert!(extends.contains(&"BaseShape"));

    let implements: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Implements).map(|r| r.to_name.as_str()).collect();
    assert!(implements.contains(&"IShape"));
}

#[test]
fn extracts_struct_and_using_directives() {
    let parsed = parse("using System;\nusing static System.Math;\n\nstruct Point {\n    void Run() {}\n}\n");
    let point = parsed.symbols.iter().find(|s| s.name == "Point").unwrap();
    assert_eq!(point.kind, SymbolKind::Struct);

    let imports: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Imports).map(|r| r.to_name.as_str()).collect();
    assert!(imports.contains(&"System"));
    assert!(imports.contains(&"Math"));
}

#[test]
fn method_end_line_is_its_closing_brace_not_the_class() {
    let parsed = parse(
        "class Foo {\n    public int Add(int a, int b) {\n        return a + b;\n    }\n}\n",
    );
    let add = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Add")
        .expect("Add method should be indexed");
    // Line 2: `public int Add(...) {`, line 4: the method's own closing `}`
    // (not line 5, the enclosing class's closing brace).
    assert_eq!(add.location.line, 2);
    assert_eq!(add.location.end_line, Some(4));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = CSharpParser.parse(&SourceFile {
        relative_path: "Broken.cs".to_string(),
        contents: "public class Broken {\n  void Go( {\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}
