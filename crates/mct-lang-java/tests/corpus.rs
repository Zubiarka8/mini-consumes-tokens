//! Long, multi-file fixture corpus (issue #74): the core of a warehouse
//! service in modern Java — money and exchange rates, the order aggregate,
//! inventory reservations, JDBC repositories and the application service —
//! each file 300–600 lines, in separate packages that import each other. The
//! shared checks (size, line ranges, golden snapshot, index round trip,
//! malformed input) come from `mct-corpus`; the tests below pin the constructs
//! and cross-file relations Java is expected to extract, and the parser's
//! documented limits.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-java/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_java::JavaParser;

mct_corpus::standard_tests!(JavaParser);

use RelationKind::{Calls, Extends, Implements, Imports};

fn parent<'a>(path: &str, name: &str, kind: SymbolKind) -> Option<&'a str> {
    corpus().symbol(path, name, kind).parent.as_deref()
}

#[test]
fn classes_interfaces_and_enums_of_every_nesting_are_extracted() {
    use SymbolKind::{Class, Enum, Interface};
    assert_eq!(parent("Money.java", "Money", Class), None);
    assert_eq!(parent("Money.java", "Currency", Enum), Some("Money"));
    assert_eq!(parent("Money.java", "Allocation", Class), Some("Money"));
    assert_eq!(
        parent("ExchangeRates.java", "ExchangeRates", Interface),
        None
    );
    assert_eq!(
        parent("ExchangeRates.java", "RateSource", Interface),
        Some("ExchangeRates")
    );
    assert_eq!(
        parent("ExchangeRates.java", "CachedRates", Class),
        Some("ExchangeRates")
    );
    assert_eq!(parent("Order.java", "State", Enum), Some("Order"));
    assert_eq!(
        parent("Inventory.java", "Listener", Interface),
        Some("Inventory")
    );
    assert_eq!(
        parent("Repository.java", "Orders", Class),
        Some("Repository")
    );
    // A local class declared inside a method body is still a class.
    assert_eq!(
        parent("Inventory.java", "Expired", Class),
        Some("Inventory")
    );
}

#[test]
fn methods_constructors_fields_and_enum_constants_attach_to_their_type() {
    use SymbolKind::{Field, Method};
    assert_eq!(parent("Money.java", "plus", Method), Some("Money"));
    assert_eq!(parent("Money.java", "scale", Field), Some("Currency"));
    assert_eq!(parent("Money.java", "JPY", Field), Some("Currency"));
    assert_eq!(parent("Order.java", "INVOICED", Field), Some("State"));
    assert_eq!(
        parent("Inventory.java", "tryReserve", Method),
        Some("StockLevel")
    );
    assert_eq!(parent("Inventory.java", "test", Method), Some("Expired"));
    assert_eq!(
        parent("ExchangeRates.java", "andThenMemo", Method),
        Some("Memo")
    );
    // Constructors are methods named after their class.
    let ctors: Vec<_> = corpus()
        .symbols_named("StockLevel")
        .into_iter()
        .map(|(_, s)| s.kind)
        .collect();
    assert_eq!(ctors, [SymbolKind::Class, Method]);
    // Overloads stay separate symbols.
    let overloads = corpus()
        .symbols_named("of")
        .into_iter()
        .filter(|(p, s)| p.ends_with("Money.java") && s.parent.as_deref() == Some("Money"))
        .count();
    assert_eq!(
        overloads, 3,
        "of(BigDecimal, …), of(String, …), of(long, …)"
    );
}

#[test]
fn enum_constant_bodies_belong_to_the_enum() {
    // `NEW { Set<State> next() { … } }`: each constant body overrides the
    // abstract `next()`; its methods, and the calls in them, belong to the
    // enum.
    let next: Vec<_> = corpus()
        .symbols_named("next")
        .into_iter()
        .filter(|(p, _)| p.ends_with("Order.java"))
        .map(|(_, s)| s.parent.as_deref())
        .collect();
    assert!(next.len() > 1, "{next:?}");
    assert!(next.iter().all(|p| *p == Some("State")), "{next:?}");
    assert!(corpus().has_relation("next", Calls, "of"));
    assert!(corpus().has_relation("next", Calls, "noneOf"));
}

#[test]
fn imports_name_the_imported_type_or_member() {
    let c = corpus();
    c.relation("Money.java", "Money", Imports, "BigDecimal");
    c.relation("Money.java", "Money", Imports, "requireNonNull");
    // Nested types are imported by their own name.
    c.relation("OrderService.java", "OrderService", Imports, "Currency");
    c.relation(
        "OrderService.java",
        "OrderService",
        Imports,
        "OutOfStockException",
    );
    c.relation(
        "OrderService.java",
        "OrderService",
        Imports,
        "Transactional",
    );
    c.relation("OrderService.java", "OrderService", Imports, "groupingBy");
}

#[test]
fn extends_and_implements_name_the_supertypes() {
    let c = corpus();
    c.relation("Money.java", "Money", Implements, "Comparable");
    c.relation(
        "Money.java",
        "CurrencyMismatchException",
        Extends,
        "IllegalArgumentException",
    );
    c.relation(
        "ExchangeRates.java",
        "CachedRates",
        Implements,
        "ExchangeRates",
    );
    c.relation(
        "ExchangeRates.java",
        "CachedRates",
        Implements,
        "AutoCloseable",
    );
    c.relation("Repository.java", "Orders", Extends, "Jdbc");
    c.relation("Repository.java", "Jdbc", Implements, "Repository");
    c.relation("Inventory.java", "Expired", Implements, "Predicate");
}

#[test]
fn calls_through_every_expression_shape_are_extracted() {
    let c = corpus();
    // Plain, qualified, chained and static calls.
    c.relation("Money.java", "plus", Calls, "requireSameCurrency");
    c.relation("Money.java", "parse", Calls, "isLetter");
    c.relation("Inventory.java", "reserveLine", Calls, "max");
    c.relation("Inventory.java", "reserveLine", Calls, "tryReserve");
    // Calls inside lambdas belong to the enclosing method.
    c.relation("Inventory.java", "reserve", Calls, "undo");
    c.relation("ExchangeRates.java", "withFallback", Calls, "eurRates");
    // Calls in a method of a local class belong to that method.
    c.relation("Inventory.java", "test", Calls, "isExpired");
}

#[test]
fn cross_file_calls_are_extracted() {
    let c = corpus();
    c.relation("Order.java", "total", Calls, "summing");
    c.relation("Order.java", "addLine", Calls, "currency");
    c.relation("Inventory.java", "reserve", Calls, "lines");
    c.relation("Repository.java", "save", Calls, "drainEvents");
    c.relation("OrderService.java", "place", Calls, "convert");
    c.relation("OrderService.java", "place", Calls, "place");
    c.relation("OrderService.java", "reserve", Calls, "reserve");
    c.relation("OrderService.java", "save", Calls, "describe");
    c.relation("OrderService.java", "demo", Calls, "identity");
    assert!(c.cross_file_relation_count() >= 100);
}

#[test]
fn records_annotation_types_and_object_creation() {
    let c = corpus();
    // `record`s are classes and `@interface`s interfaces…
    for name in ["Range", "Line", "Placed", "Reservation", "Page"] {
        assert!(
            c.symbols_named(name)
                .iter()
                .any(|(_, s)| s.kind == SymbolKind::Class),
            "record {name}"
        );
    }
    for name in ["ThreadSafe", "Transactional"] {
        assert!(
            c.symbols_named(name)
                .iter()
                .any(|(_, s)| s.kind == SymbolKind::Interface),
            "@interface {name}"
        );
    }
    // …so a record's methods attach to the record…
    assert_eq!(
        parent("Money.java", "contains", SymbolKind::Method),
        Some("Range")
    );
    assert_eq!(
        parent("Order.java", "subtotal", SymbolKind::Method),
        Some("Line")
    );
    // …and its `implements` is a relation.
    c.relation("Order.java", "Placed", Implements, "OrderEvent");
    // `new T(…)` calls the type.
    assert!(c.has_relation("place", Calls, "PendingApproval"));
    // Known limit (module doc): an anonymous class's methods attach to the
    // enclosing type, and a sealed interface's `permits` is no relation.
    assert_eq!(
        parent("OrderService.java", "run", SymbolKind::Method),
        Some("OrderService")
    );
}

#[test]
#[ignore = "bug, not filed yet: `this(…)`/`super(…)` and method references (`Money::plus`) leave no relation"]
fn constructor_chaining_and_method_references_are_relations() {
    assert!(corpus().has_relation("summing", Calls, "plus"));
}

#[test]
fn type_arguments_and_qualifiers_in_supertypes_are_not_supertypes() {
    let c = corpus();
    c.relation("Money.java", "Money", Implements, "Comparable");
    assert!(!c.has_relation("Money", Implements, "Money"));
    c.relation("Repository.java", "Orders", Extends, "Jdbc");
    assert!(!c.has_relation("Orders", Extends, "Order"));
    assert!(!c.has_relation("Memo", Implements, "K"));
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    let callers = index.find_callers("summing").unwrap();
    let mut files: Vec<_> = callers.iter().map(|h| h.relative_path.as_str()).collect();
    files.sort();
    files.dedup();
    for file in [
        "com/example/warehouse/money/Money.java",
        "com/example/warehouse/order/Order.java",
        "com/example/warehouse/persistence/Repository.java",
    ] {
        assert!(files.contains(&file), "summing should be called in {file}");
    }
    let calls = index.find_calls("place").unwrap();
    for callee in ["priceLines", "convert", "reserve", "save", "cancel"] {
        assert!(
            calls.iter().any(|h| h.to_name == callee),
            "place should call {callee}"
        );
    }
}
