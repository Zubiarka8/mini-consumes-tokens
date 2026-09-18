// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-java/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_java::JavaParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    JavaParser
        .parse(&SourceFile {
            relative_path: "Calculator.java".to_string(),
            contents: src.to_string(),
        })
        .expect("valid Java source should parse")
}

#[test]
fn extracts_class_method_and_call() {
    let parsed = parse(
        "public class Calculator {\n    public int add(int a, int b) {\n        return helper(a, b);\n    }\n\n    private int helper(int a, int b) {\n        return a + b;\n    }\n}\n",
    );
    // "Calculator" is ambiguous by name alone: the file-level module
    // pseudo-symbol (named after Calculator.java) collides with the class
    // of the same name — the idiomatic Java convention of one public type
    // per file, named after it. Both legitimately exist; disambiguate by kind.
    assert!(parsed.symbols.iter().any(|s| s.name == "Calculator" && s.kind == SymbolKind::Class));

    let helper = parsed.symbols.iter().find(|s| s.name == "helper").unwrap();
    assert_eq!(helper.kind, SymbolKind::Method);
    assert_eq!(helper.parent.as_deref(), Some("Calculator"));

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"helper"));
}

#[test]
fn overloaded_methods_are_kept_as_separate_symbols() {
    let parsed = parse(
        "public class Calculator {\n    public int add(int a, int b) {\n        return a + b;\n    }\n\n    public double add(double a, double b) {\n        return a + b;\n    }\n}\n",
    );
    let adds: Vec<_> = parsed.symbols.iter().filter(|s| s.name == "add").collect();
    assert_eq!(adds.len(), 2, "both overloads of add() should be indexed as distinct symbols");
    assert_ne!(
        adds[0].location.line, adds[1].location.line,
        "overloads must keep distinct locations, not collapse into one"
    );
    assert!(adds.iter().all(|s| s.kind == SymbolKind::Method));
    assert!(adds.iter().all(|s| s.parent.as_deref() == Some("Calculator")));
}

#[test]
fn nested_class_is_not_lost_or_confused_with_outer() {
    let parsed = parse(
        "public class Outer {\n    public static class Inner {\n        void go() {}\n    }\n\n    void outerMethod() {}\n}\n",
    );
    let inner = parsed.symbols.iter().find(|s| s.name == "Inner").unwrap();
    assert_eq!(inner.kind, SymbolKind::Class);
    assert_eq!(inner.parent.as_deref(), Some("Outer"));

    let go = parsed.symbols.iter().find(|s| s.name == "go").unwrap();
    assert_eq!(go.parent.as_deref(), Some("Inner"), "Inner's method must not be attributed to Outer");

    let outer_method = parsed.symbols.iter().find(|s| s.name == "outerMethod").unwrap();
    assert_eq!(outer_method.parent.as_deref(), Some("Outer"));
}

#[test]
fn extracts_interface_implements_and_extends() {
    let parsed = parse(
        "public interface Shape {\n    double area();\n}\n\npublic class Circle extends BaseShape implements Shape {\n    public double area() {\n        return 0.0;\n    }\n}\n",
    );
    let shape = parsed.symbols.iter().find(|s| s.name == "Shape").unwrap();
    assert_eq!(shape.kind, SymbolKind::Interface);

    let extends: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Extends).map(|r| r.to_name.as_str()).collect();
    assert!(extends.contains(&"BaseShape"));

    let implements: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Implements).map(|r| r.to_name.as_str()).collect();
    assert!(implements.contains(&"Shape"));
}

#[test]
fn extracts_imports_including_static() {
    let parsed = parse(
        "import java.util.List;\nimport static java.lang.Math.max;\n\nclass A {}\n",
    );
    let imports: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Imports).map(|r| r.to_name.as_str()).collect();
    assert!(imports.contains(&"List"));
    assert!(imports.contains(&"max"));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = JavaParser.parse(&SourceFile {
        relative_path: "Broken.java".to_string(),
        contents: "public class Broken {\n  void go( {\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}
