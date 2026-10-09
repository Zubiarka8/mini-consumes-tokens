// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-kotlin/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_kotlin::KotlinParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    KotlinParser
        .parse(&SourceFile {
            relative_path: "Invoice.kt".to_string(),
            contents: src.to_string(),
        })
        .expect("valid Kotlin source should parse")
}

#[test]
fn file_module_ends_on_the_last_line() {
    // A trailing newline must not push the module one line past the file.
    let module = |src: &str| {
        let parsed = parse(src);
        let m = parsed
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module && s.name == "Invoice")
            .unwrap();
        (m.location.line, m.location.end_line)
    };
    let class = "class A {\n  fun f() {}\n}";
    assert_eq!(module(&format!("{class}\n")), (1, Some(3)));
    assert_eq!(module(class), (1, Some(3)));
    assert_eq!(module("class A\n"), (1, Some(1)));
}

#[test]
fn extracts_class_method_and_call() {
    let parsed = parse(
        "class Invoice {\n    fun total(): Int {\n        return helper()\n    }\n\n    fun helper(): Int {\n        return 42\n    }\n}\n",
    );
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == "Invoice" && s.kind == SymbolKind::Class));

    let helper = parsed.symbols.iter().find(|s| s.name == "helper").unwrap();
    assert_eq!(helper.kind, SymbolKind::Method);
    assert_eq!(helper.parent.as_deref(), Some("Invoice"));

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"helper"));
}

#[test]
fn interface_is_distinguished_from_class() {
    let parsed = parse("interface Priceable {\n    fun price(): Double\n}\n\nclass Item {\n    fun name(): String = \"x\"\n}\n");
    let priceable = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Priceable")
        .unwrap();
    assert_eq!(priceable.kind, SymbolKind::Interface);

    let item = parsed.symbols.iter().find(|s| s.name == "Item").unwrap();
    assert_eq!(item.kind, SymbolKind::Class);
}

#[test]
fn object_declaration_is_indexed_as_singleton_class() {
    let parsed = parse(
        "object Logger {\n    fun log(message: String) {\n        println(message)\n    }\n}\n",
    );
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
    let extends: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Extends)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(extends.contains(&"Shape"));

    let implements: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Implements)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(implements.contains(&"Priceable"));
    assert!(
        !implements.contains(&"Shape"),
        "the superclass call must not also be recorded as Implements"
    );
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
    let parsed =
        parse("import java.math.BigDecimal\nimport java.time.LocalDate as Date\n\nclass A\n");
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"BigDecimal"));
    assert!(
        imports.contains(&"Date"),
        "an aliased import should be indexed under its alias, not its original name"
    );
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = KotlinParser.parse(&SourceFile {
        relative_path: "Broken.kt".to_string(),
        contents: "class Broken {\n    fun go( {\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}

fn symbol<'a>(parsed: &'a mct_core::ParsedFile, name: &str) -> &'a mct_core::SymbolRecord {
    parsed
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol {name}"))
}

#[test]
fn top_level_and_local_functions_are_functions() {
    let parsed = parse(
        "fun vat(amount: Int): Int {\n    fun rule(x: Int) = x * 2\n    return rule(amount)\n}\n\nclass Pricing {\n    fun quote(): Int {\n        val subtotal = vat(1)\n        fun round(x: Int) = x\n        return round(subtotal)\n    }\n}\n",
    );
    for name in ["vat", "rule", "round"] {
        let f = symbol(&parsed, name);
        assert_eq!(
            (f.kind, f.parent.as_deref()),
            (SymbolKind::Function, None),
            "{name}"
        );
    }
    let quote = symbol(&parsed, "quote");
    assert_eq!(
        (quote.kind, quote.parent.as_deref()),
        (SymbolKind::Method, Some("Pricing"))
    );
    // A local `val` is not a field of the class.
    assert!(parsed.symbols.iter().all(|s| s.name != "subtotal"));
}

#[test]
fn an_extension_receiver_drops_its_type_arguments() {
    let parsed = parse(
        "fun Collection<Order>.countByState(): Map<State, Int> = groupingBy { it.state }.eachCount()\n",
    );
    let f = symbol(&parsed, "countByState");
    assert_eq!(
        (f.kind, f.parent.as_deref()),
        (SymbolKind::Method, Some("Collection"))
    );
}

#[test]
fn enum_class_bodies_are_visited() {
    let parsed = parse(
        "enum class State {\n    NEW {\n        override fun next() = setOf(PAID)\n    },\n    PAID {\n        override fun next() = emptySet<State>()\n    };\n\n    abstract fun next(): Set<State>\n    fun isTerminal() = next().isEmpty()\n}\n",
    );
    for entry in ["NEW", "PAID"] {
        let e = symbol(&parsed, entry);
        assert_eq!(
            (e.kind, e.parent.as_deref()),
            (SymbolKind::Field, Some("State"))
        );
    }
    let terminal = symbol(&parsed, "isTerminal");
    assert_eq!(terminal.parent.as_deref(), Some("State"));
    assert_eq!(
        parsed.symbols.iter().filter(|s| s.name == "next").count(),
        3
    );
    assert!(parsed
        .relations
        .iter()
        .any(|r| r.kind == RelationKind::Calls && r.to_name == "setOf"));
}

#[test]
fn pathological_error_recovery_stops_at_the_parse_budget() {
    // Fuzz inputs on which tree-sitter's error recovery needed over 1 GB and a
    // minute (CI fuzz OOMs, PR #157), originals and minimized. The fuzz
    // smoke job replays the same files under AddressSanitizer.
    let inputs: [&[u8]; 4] = [
        include_bytes!(
            "../fuzz/regressions/parse-kotlin/oom-3030c6b6d52561c8d49bb812d683bce15abc7ada"
        ),
        include_bytes!("../fuzz/regressions/parse-kotlin/oom-3030c6b6-minimized"),
        include_bytes!(
            "../fuzz/regressions/parse-kotlin/oom-e6ff4d3a0f18595d5f74790c2521c41b441c4f51"
        ),
        include_bytes!("../fuzz/regressions/parse-kotlin/oom-e6ff4d3a-minimized"),
    ];
    for input in inputs {
        let result = KotlinParser.parse(&SourceFile {
            relative_path: "Fuzz.kt".to_string(),
            contents: String::from_utf8(input.to_vec()).unwrap(),
        });
        match result {
            Err(mct_core::ParseError::Syntax { message, .. }) => {
                assert!(message.contains("budget"), "{message}");
            }
            other => panic!("expected a budget syntax error, got {other:?}"),
        }
    }
}
