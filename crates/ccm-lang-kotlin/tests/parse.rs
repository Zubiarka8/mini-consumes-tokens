// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/ccm-lang-kotlin/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ccm_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use ccm_lang_kotlin::KotlinParser;

fn parse(src: &str) -> ccm_core::ParsedFile {
    KotlinParser
        .parse(&SourceFile { relative_path: "Invoice.kt".to_string(), contents: src.to_string() })
        .expect("valid Kotlin source should parse")
}

#[test]
fn extracts_class_method_and_call() {
    let parsed = parse(
        "class Invoice {\n    fun total(): Int {\n        return helper()\n    }\n\n    fun helper(): Int {\n        return 42\n    }\n}\n",
    );
    assert!(parsed.symbols.iter().any(|s| s.name == "Invoice" && s.kind == SymbolKind::Class));

    let helper = parsed.symbols.iter().find(|s| s.name == "helper").unwrap();
    assert_eq!(helper.kind, SymbolKind::Method);
    assert_eq!(helper.parent.as_deref(), Some("Invoice"));

    let calls: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Calls).map(|r| r.to_name.as_str()).collect();
    assert!(calls.contains(&"helper"));
}

#[test]
fn interface_is_distinguished_from_class() {
    let parsed = parse("interface Priceable {\n    fun price(): Double\n}\n\nclass Item {\n    fun name(): String = \"x\"\n}\n");
    let priceable = parsed.symbols.iter().find(|s| s.name == "Priceable").unwrap();
    assert_eq!(priceable.kind, SymbolKind::Interface);

    let item = parsed.symbols.iter().find(|s| s.name == "Item").unwrap();
    assert_eq!(item.kind, SymbolKind::Class);
}

#[test]
fn object_declaration_is_indexed_as_singleton_class() {
    let parsed = parse("object Logger {\n    fun log(message: String) {\n        println(message)\n    }\n}\n");
    let logger = parsed.symbols.iter().find(|s| s.name == "Logger").unwrap();
    assert_eq!(logger.kind, SymbolKind::Class);

    let log = parsed.symbols.iter().find(|s| s.name == "log").unwrap();
    assert_eq!(log.parent.as_deref(), Some("Logger"));
}

#[test]
fn extends_and_implements_are_distinguished_by_constructor_call() {
    // Kotlin has no `extends`/`implements` keywords: a supertype entry with
    // a constructor call (`Shape(4)`) is the superclass, a bare type name
    // (`Priceable`) is an interface — verified via the real grammar's
    // constructor_invocation vs. bare user_type node kinds, not a heuristic.
    let parsed = parse(
        "open class Shape(val sides: Int)\n\ninterface Priceable {\n    fun price(): Double\n}\n\nclass Circle(radius: Int) : Shape(4), Priceable {\n    fun price(): Double = 1.0\n}\n",
    );
    let extends: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Extends).map(|r| r.to_name.as_str()).collect();
    assert!(extends.contains(&"Shape"));

    let implements: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Implements).map(|r| r.to_name.as_str()).collect();
    assert!(implements.contains(&"Priceable"));
    assert!(!implements.contains(&"Shape"), "the superclass call must not also be recorded as Implements");
}

#[test]
fn extension_function_attaches_to_its_receiver_type() {
    // Idiomatic Kotlin: an extension function's "self" is the receiver
    // type, not the file/class it's textually written in — same modeling
    // choice as Go's pointer-receiver methods.
    let parsed = parse("fun String.shout(): String = this.uppercase()\n");
    let shout = parsed.symbols.iter().find(|s| s.name == "shout").unwrap();
    assert_eq!(shout.kind, SymbolKind::Method);
    assert_eq!(shout.parent.as_deref(), Some("String"));
}

#[test]
fn primary_constructor_property_promotion_is_indexed_as_fields() {
    // Idiomatic Kotlin: `val`/`var` directly in the primary constructor
    // parameter list declares a property, not just a constructor parameter.
    let parsed = parse("class LineItem(val name: String, private val amount: Int, count: Int)\n");
    let name_field = parsed.symbols.iter().find(|s| s.name == "name").unwrap();
    assert_eq!(name_field.kind, SymbolKind::Field);
    assert_eq!(name_field.parent.as_deref(), Some("LineItem"));

    let amount_field = parsed.symbols.iter().find(|s| s.name == "amount").unwrap();
    assert_eq!(amount_field.kind, SymbolKind::Field);

    assert!(
        !parsed.symbols.iter().any(|s| s.name == "count"),
        "a plain constructor parameter without val/var must not become a Field"
    );
}

#[test]
fn extracts_imports_with_and_without_alias() {
    let parsed = parse("import java.math.BigDecimal\nimport java.time.LocalDate as Date\n\nclass A\n");
    let imports: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Imports).map(|r| r.to_name.as_str()).collect();
    assert!(imports.contains(&"BigDecimal"));
    assert!(imports.contains(&"Date"), "an aliased import should be indexed under its alias, not its original name");
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = KotlinParser.parse(&SourceFile {
        relative_path: "Broken.kt".to_string(),
        contents: "class Broken {\n    fun go( {\n".to_string(),
    });
    assert!(matches!(result, Err(ccm_core::ParseError::Syntax { .. })));
}
