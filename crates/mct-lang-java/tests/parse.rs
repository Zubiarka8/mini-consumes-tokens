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
fn file_module_ends_on_the_last_line() {
    // A trailing newline must not push the module one line past the file.
    let module = |src: &str| {
        let parsed = parse(src);
        let m = parsed
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module)
            .unwrap();
        (m.location.line, m.location.end_line)
    };
    let class = "class A {\n  void f() {}\n}";
    assert_eq!(module(&format!("{class}\n")), (1, Some(3)));
    assert_eq!(module(class), (1, Some(3)));
    assert_eq!(module("class A {}\n"), (1, Some(1)));
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
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == "Calculator" && s.kind == SymbolKind::Class));

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
    assert_eq!(
        adds.len(),
        2,
        "both overloads of add() should be indexed as distinct symbols"
    );
    assert_ne!(
        adds[0].location.line, adds[1].location.line,
        "overloads must keep distinct locations, not collapse into one"
    );
    assert!(adds.iter().all(|s| s.kind == SymbolKind::Method));
    assert!(adds
        .iter()
        .all(|s| s.parent.as_deref() == Some("Calculator")));
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
    assert_eq!(
        go.parent.as_deref(),
        Some("Inner"),
        "Inner's method must not be attributed to Outer"
    );

    let outer_method = parsed
        .symbols
        .iter()
        .find(|s| s.name == "outerMethod")
        .unwrap();
    assert_eq!(outer_method.parent.as_deref(), Some("Outer"));
}

#[test]
fn extracts_interface_implements_and_extends() {
    let parsed = parse(
        "public interface Shape {\n    double area();\n}\n\npublic class Circle extends BaseShape implements Shape {\n    public double area() {\n        return 0.0;\n    }\n}\n",
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
}

fn supertypes(parsed: &mct_core::ParsedFile, kind: RelationKind) -> Vec<&str> {
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| r.to_name.as_str())
        .collect()
}

#[test]
fn type_arguments_and_qualifiers_are_not_supertypes() {
    let parsed = parse(
        "class Money implements Comparable<Money>, java.io.Serializable {}\n\
         class Orders extends Jdbc<Order, Long> implements Repository<Map<String, Order>> {}\n",
    );
    assert_eq!(
        supertypes(&parsed, RelationKind::Implements),
        ["Comparable", "Serializable", "Repository"]
    );
    assert_eq!(supertypes(&parsed, RelationKind::Extends), ["Jdbc"]);
}

#[test]
fn enum_constant_bodies_are_visited() {
    let parsed = parse(
        "enum State {\n    NEW {\n        Set<State> next() {\n            return EnumSet.of(PAID);\n        }\n    },\n    PAID;\n    abstract Set<State> next();\n}\n",
    );
    let next: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.name == "next")
        .map(|s| (s.location.line, s.parent.as_deref()))
        .collect();
    assert_eq!(next, [(3, Some("State")), (8, Some("State"))]);
    let of = parsed
        .relations
        .iter()
        .find(|r| r.to_name == "of")
        .expect("call in the constant body");
    let from = parsed.symbols.iter().find(|s| s.id == of.from).unwrap();
    assert_eq!((from.name.as_str(), from.location.line), ("next", 3));
}

#[test]
fn records_and_annotation_types_are_type_symbols() {
    let parsed = parse(
        "class Money {\n    record Range(Money min, Money max) implements Comparable<Range> {\n        boolean contains(Money m) { return check(m); }\n    }\n    @interface ThreadSafe {}\n}\n",
    );
    let range = parsed.symbols.iter().find(|s| s.name == "Range").unwrap();
    assert_eq!(range.kind, SymbolKind::Class);
    assert_eq!(range.parent.as_deref(), Some("Money"));
    let contains = parsed
        .symbols
        .iter()
        .find(|s| s.name == "contains")
        .unwrap();
    assert_eq!(contains.parent.as_deref(), Some("Range"));
    assert_eq!(
        supertypes(&parsed, RelationKind::Implements),
        ["Comparable"]
    );
    let safe = parsed
        .symbols
        .iter()
        .find(|s| s.name == "ThreadSafe")
        .unwrap();
    assert_eq!(safe.kind, SymbolKind::Interface);
}

#[test]
fn object_creation_calls_the_created_type() {
    let parsed = parse(
        "class S {\n    void place() {\n        var e = new PendingApproval(id);\n        var m = new java.util.HashMap<String, Order>();\n    }\n}\n",
    );
    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| (r.to_name.as_str(), r.location.line))
        .collect();
    assert_eq!(calls, [("PendingApproval", 3), ("HashMap", 4)]);
}

#[test]
fn extracts_imports_including_static() {
    let parsed = parse("import java.util.List;\nimport static java.lang.Math.max;\n\nclass A {}\n");
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
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
