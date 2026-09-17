// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/ccm-lang-php/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ccm_core::{LanguageParser, ParsedFile, RelationKind, SourceFile, SymbolKind};
use ccm_lang_php::PhpParser;

fn parse(src: &str) -> ParsedFile {
    PhpParser
        .parse(&SourceFile { relative_path: "fixture.php".to_string(), contents: src.to_string() })
        .expect("valid php source should parse")
}

#[test]
fn extracts_class_extends_and_implements() {
    let parsed = parse(
        "<?php\ninterface Payable { public function pay(): bool; }\nclass Invoice extends Base implements Payable {}\n",
    );
    let invoice = parsed.symbols.iter().find(|s| s.name == "Invoice").unwrap();
    assert_eq!(invoice.kind, SymbolKind::Class);

    let extends: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Extends).map(|r| r.to_name.as_str()).collect();
    assert!(extends.contains(&"Base"));

    let implements: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Implements).map(|r| r.to_name.as_str()).collect();
    assert!(implements.contains(&"Payable"));
}

#[test]
fn methods_and_fields_are_parented_to_their_class() {
    let parsed = parse("<?php\nclass Invoice {\n  public float $amount;\n  public function pay(): bool { return true; }\n}\n");
    let pay = parsed.symbols.iter().find(|s| s.name == "pay").unwrap();
    assert_eq!(pay.kind, SymbolKind::Method);
    assert_eq!(pay.parent.as_deref(), Some("Invoice"));

    let amount = parsed.symbols.iter().find(|s| s.name == "amount").unwrap();
    assert_eq!(amount.kind, SymbolKind::Field);
    assert_eq!(amount.parent.as_deref(), Some("Invoice"));
}

#[test]
fn constructor_property_promotion_is_indexed_as_a_field() {
    let parsed = parse("<?php\nclass Invoice {\n  public function __construct(private readonly string $id, public int $amount = 0) {}\n}\n");
    let id_field = parsed.symbols.iter().find(|s| s.name == "id").unwrap();
    assert_eq!(id_field.kind, SymbolKind::Field);
    assert_eq!(id_field.parent.as_deref(), Some("Invoice"));

    let amount_field = parsed.symbols.iter().find(|s| s.name == "amount").unwrap();
    assert_eq!(amount_field.kind, SymbolKind::Field);
    assert_eq!(amount_field.parent.as_deref(), Some("Invoice"));
}

#[test]
fn trait_use_is_recorded_as_implements_on_the_class() {
    let parsed = parse(
        "<?php\ntrait Loggable { public function log(string $m): void {} }\nclass Invoice {\n  use Loggable;\n  public function noop(): void {}\n}\n",
    );
    let loggable = parsed.symbols.iter().find(|s| s.name == "Loggable").unwrap();
    assert_eq!(loggable.kind, SymbolKind::Trait);

    let invoice = parsed.symbols.iter().find(|s| s.name == "Invoice").unwrap();
    let uses: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.from == invoice.id && r.kind == RelationKind::Implements)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(uses.contains(&"Loggable"));
}

#[test]
fn trait_use_with_adaptation_block_does_not_leak_adaptation_names() {
    let parsed = parse(
        "<?php\ntrait A { public function foo(): void {} }\ntrait B { public function foo(): void {} }\nclass C {\n  use A, B {\n    A::foo insteadof B;\n    B::foo as protected bar;\n  }\n}\n",
    );
    let c = parsed.symbols.iter().find(|s| s.name == "C").unwrap();
    let uses: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.from == c.id && r.kind == RelationKind::Implements)
        .map(|r| r.to_name.as_str())
        .collect();
    assert_eq!(uses.len(), 2, "only A and B, not names from the insteadof/as adaptation block: {uses:?}");
    assert!(uses.contains(&"A"));
    assert!(uses.contains(&"B"));
}

#[test]
fn scoped_and_member_calls_are_all_recorded() {
    let parsed = parse(
        "<?php\nclass Invoice extends Base {\n  public function process(): void {\n    $this->log('x');\n    parent::process();\n    self::validate();\n    static::hook();\n    Invoice::create();\n    helper();\n  }\n}\n",
    );
    let calls: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Calls).map(|r| r.to_name.as_str()).collect();
    for expected in ["log", "process", "validate", "hook", "create", "helper"] {
        assert!(calls.contains(&expected), "missing call to {expected}: {calls:?}");
    }
}

#[test]
fn enum_case_and_backed_enum_methods_are_indexed() {
    let parsed = parse(
        "<?php\nenum Suit: string {\n  case Hearts = 'H';\n  public function label(): string { return 'x'; }\n}\n",
    );
    let hearts = parsed.symbols.iter().find(|s| s.name == "Hearts").unwrap();
    assert_eq!(hearts.kind, SymbolKind::Field);
    assert_eq!(hearts.parent.as_deref(), Some("Suit"));

    let label = parsed.symbols.iter().find(|s| s.name == "label").unwrap();
    assert_eq!(label.kind, SymbolKind::Method);
    assert_eq!(label.parent.as_deref(), Some("Suit"));
}

#[test]
fn top_level_and_class_constants_are_indexed() {
    let parsed = parse("<?php\nconst LIMIT = 100;\nclass Invoice {\n  const VERSION = '1.0';\n}\n");
    let limit = parsed.symbols.iter().find(|s| s.name == "LIMIT").unwrap();
    assert_eq!(limit.kind, SymbolKind::Constant);

    let version = parsed.symbols.iter().find(|s| s.name == "VERSION").unwrap();
    assert_eq!(version.kind, SymbolKind::Constant);
    assert_eq!(version.parent.as_deref(), Some("Invoice"));
}

#[test]
fn anonymous_and_arrow_functions_assigned_to_a_variable_are_indexed() {
    let parsed = parse("<?php\n$formatter = function (float $x): string { return number_format($x, 2); };\n$adder = fn($a, $b) => $a + $b;\n");
    let formatter = parsed.symbols.iter().find(|s| s.name == "formatter").unwrap();
    assert_eq!(formatter.kind, SymbolKind::Function);

    let adder = parsed.symbols.iter().find(|s| s.name == "adder").unwrap();
    assert_eq!(adder.kind, SymbolKind::Function);

    let calls: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Calls).map(|r| r.to_name.as_str()).collect();
    assert!(calls.contains(&"number_format"), "call inside the closure body must still be recorded: {calls:?}");
}

#[test]
fn use_and_require_are_imports() {
    let parsed = parse(
        "<?php\nnamespace App;\nuse App\\Models\\User;\nuse App\\Models\\{Order, Invoice as Inv};\nrequire_once 'bootstrap.php';\nrequire __DIR__ . '/computed.php';\n",
    );
    let imports: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Imports).map(|r| r.to_name.as_str()).collect();
    assert!(imports.contains(&"User"));
    assert!(imports.contains(&"Order"));
    assert!(imports.contains(&"Inv"), "aliased import is recorded under its alias");
    assert!(imports.contains(&"bootstrap.php"));
    assert!(!imports.contains(&"__DIR__"), "a computed require path has nothing static to record");
}

#[test]
fn define_call_is_an_ordinary_call_not_a_constant_symbol() {
    let parsed = parse("<?php\ndefine('LEGACY_FLAG', true);\n");
    assert!(parsed.symbols.iter().all(|s| s.name != "LEGACY_FLAG"), "define() arguments must not synthesize a symbol");
    let calls: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Calls).map(|r| r.to_name.as_str()).collect();
    assert!(calls.contains(&"define"));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = PhpParser.parse(&SourceFile {
        relative_path: "broken.php".to_string(),
        contents: "<?php\nclass Foo {\n".to_string(),
    });
    assert!(matches!(result, Err(ccm_core::ParseError::Syntax { .. })));
}

// Regression test for a real crash: CI's fuzz-smoke job found a native
// stack-overflow (AddressSanitizer) from adversarially deep nesting, since
// the Walker's visit/visit_children recursion had no depth limit. This
// input is exactly the shape that triggered it — thousands of nested
// parenthesized expressions — and must return `Ok` (truncated past
// `MAX_TRAVERSAL_DEPTH`, never crashing the process) rather than panic.
#[test]
fn deeply_nested_expression_does_not_stack_overflow() {
    let nesting = 5_000;
    let mut source = String::from("<?php\n$x = ");
    source.push_str(&"(".repeat(nesting));
    source.push('1');
    source.push_str(&")".repeat(nesting));
    source.push_str(";\n");

    let result = PhpParser.parse(&SourceFile { relative_path: "deep.php".to_string(), contents: source });
    assert!(result.is_ok(), "deeply nested input must not crash the parser, even if truncated");
}
