//! Long, multi-file fixture corpus (issue #74): a warehouse service in
//! idiomatic Kotlin — money (value-like class, enum with bodies, operators,
//! extensions, delegation), the order aggregate (sealed events, data classes,
//! validation), inventory (coroutines, delegated properties), pricing (a DSL
//! with lambdas with receiver, sealed promotions) and the application wiring
//! (router, objects, `main`) — each file 300–600 lines and importing the
//! others. The shared checks (size, line ranges, golden snapshot, index round
//! trip, malformed input) come from `mct-corpus`; the tests below pin what
//! Kotlin is expected to extract and the parser's documented limits.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-kotlin/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_kotlin::KotlinParser;

mct_corpus::standard_tests!(KotlinParser);

use RelationKind::{Calls, Extends, Implements, Imports};
use SymbolKind::{Class, Field, Interface, Method, Module};

fn parent<'a>(path: &str, name: &str, kind: SymbolKind) -> Option<&'a str> {
    corpus().symbol(path, name, kind).parent.as_deref()
}

#[test]
fn package_headers_are_modules() {
    for (path, package) in [
        ("Money.kt", "com.example.warehouse.money"),
        ("Order.kt", "com.example.warehouse.order"),
        ("WarehouseApp.kt", "com.example.warehouse.app"),
    ] {
        let m = corpus().symbol(path, package, Module);
        // Limit: the package module spans the header line only, unlike Go's.
        assert_eq!(
            (m.location.line, m.location.end_line),
            (1, Some(1)),
            "{path}"
        );
    }
}

#[test]
fn classes_objects_and_interfaces_are_extracted() {
    assert_eq!(parent("Money.kt", "Money", Class), None);
    assert_eq!(parent("Money.kt", "MoneyRange", Class), None);
    assert_eq!(parent("Money.kt", "Currency", Class), None);
    assert_eq!(parent("Money.kt", "ExchangeRates", Interface), None);
    assert_eq!(parent("Order.kt", "OrderId", Class), None);
    assert_eq!(parent("Order.kt", "Placed", Class), Some("OrderEvent"));
    assert_eq!(parent("Inventory.kt", "InventoryListener", Interface), None);
    assert_eq!(parent("Pricing.kt", "BuyXGetY", Class), Some("Promotion"));
    // `object` declarations are classes too.
    assert_eq!(
        parent("Pricing.kt", "FreeShipping", Class),
        Some("Promotion")
    );
    assert_eq!(parent("WarehouseApp.kt", "OrderNumbers", Class), None);
}

#[test]
fn members_attach_to_their_type() {
    assert_eq!(parent("Money.kt", "plus", Method), Some("Money"));
    assert_eq!(parent("Money.kt", "amount", Field), Some("Money"));
    assert_eq!(
        parent("Inventory.kt", "tryReserve", Method),
        Some("StockLevel")
    );
    assert_eq!(
        parent("Pricing.kt", "volume", Method),
        Some("PricingBuilder")
    );
    assert_eq!(parent("WarehouseApp.kt", "handle", Method), Some("Router"));
    // Primary-constructor `val`/`var` parameters are fields.
    assert_eq!(
        parent("Inventory.kt", "orderId", Field),
        Some("Reservation")
    );
    assert_eq!(
        parent("Pricing.kt", "customerTier", Field),
        Some("PricingContext")
    );
}

#[test]
fn extension_functions_attach_to_their_receiver() {
    assert_eq!(parent("Money.kt", "toMoney", Method), Some("String"));
    // `fun Int.eur()` and `fun BigDecimal.eur()` are two symbols.
    let eur: Vec<_> = corpus()
        .symbols_named("eur")
        .into_iter()
        .map(|(_, s)| s.parent.as_deref().unwrap())
        .collect();
    assert_eq!(eur, ["Int", "BigDecimal"]);
    // Bug, kept visible: the receiver keeps its type arguments.
    assert_eq!(
        parent("Order.kt", "countByState", Method),
        Some("Collection<Order>")
    );
    assert_eq!(
        parent("Inventory.kt", "pickingRoute", Method),
        Some("List<Location>")
    );
}

#[test]
fn delegation_specifiers_split_into_extends_and_implements() {
    let c = corpus();
    // A constructor call means a superclass…
    c.relation(
        "Money.kt",
        "CurrencyMismatchException",
        Extends,
        "IllegalArgumentException",
    );
    c.relation("Order.kt", "Placed", Extends, "OrderEvent");
    c.relation("Pricing.kt", "BuyXGetY", Extends, "Promotion");
    // …a bare type an interface, type arguments dropped.
    c.relation("Money.kt", "Money", Implements, "Comparable");
    c.relation("Inventory.kt", "Location", Implements, "Comparable");
    c.relation("WarehouseApp.kt", "Outbox", Implements, "EventSink");
    c.relation("WarehouseApp.kt", "Outbox", Implements, "InventoryListener");
    assert!(!c.has_relation("Money", Implements, "Money"));
}

#[test]
fn imports_name_the_imported_symbol_or_its_alias() {
    let c = corpus();
    c.relation("Order.kt", "Order", Imports, "sum");
    c.relation("WarehouseApp.kt", "WarehouseApp", Imports, "defaultEngine");
    // `import java.util.Currency as JavaCurrency`.
    c.relation("Money.kt", "Money", Imports, "JavaCurrency");
    c.relation("WarehouseApp.kt", "WarehouseApp", Imports, "renderSlip");
}

#[test]
fn calls_through_every_expression_shape_are_extracted() {
    let c = corpus();
    // Constructor calls, safe calls, trailing lambdas, scope functions.
    c.relation("Money.kt", "plus", Calls, "requireSameCurrency");
    c.relation("Money.kt", "plus", Calls, "Money");
    c.relation("Inventory.kt", "reserveLine", Calls, "tryReserve");
    c.relation("Inventory.kt", "reserveLine", Calls, "maxByOrNull");
    c.relation("Pricing.kt", "quote", Calls, "distributeDiscount");
    // Calls in a `suspend` function and inside `withLock { … }`.
    c.relation("Inventory.kt", "reserve", Calls, "reserveLine");
    c.relation("Inventory.kt", "reserve", Calls, "withLock");
}

#[test]
fn cross_file_calls_are_extracted() {
    let c = corpus();
    c.relation("Order.kt", "place", Calls, "random");
    c.relation("Pricing.kt", "quote", Calls, "convert");
    c.relation("Pricing.kt", "shelfPrice", Calls, "gross");
    c.relation("WarehouseApp.kt", "place", Calls, "quote");
    c.relation("WarehouseApp.kt", "reserveOrReject", Calls, "reserve");
    c.relation("WarehouseApp.kt", "demoService", Calls, "defaultEngine");
    assert!(c.cross_file_relation_count() >= 80);
}

#[test]
fn enum_class_bodies_are_not_visited() {
    // Bug, kept visible: an `enum class` body (`enum_class_body`) is skipped,
    // so its entries, methods, properties and companion are lost.
    let c = corpus();
    for name in [
        "canMoveTo",
        "isTerminal",
        "javaCurrency",
        "forSku",
        "EUR",
        "GENERAL",
    ] {
        assert!(c.symbols_named(name).is_empty(), "{name} was extracted");
    }
    assert!(!c.has_relation("next", Calls, "setOf"));
    // The enum itself and its constructor properties are still there.
    assert_eq!(parent("Money.kt", "scale", Field), Some("Currency"));
    assert_eq!(parent("Pricing.kt", "VatCategory", Class), None);
}

#[test]
fn documented_limits_of_the_parser() {
    let c = corpus();
    // Top-level functions are methods without a parent, not functions.
    assert_eq!(parent("Money.kt", "vat", Method), None);
    assert!(c
        .symbols_named("auditLine")
        .iter()
        .all(|(_, s)| s.kind == Method && s.parent.is_none()));
    // Local functions are methods too, owned by the enclosing type (or none).
    assert_eq!(parent("Order.kt", "rule", Method), None);
    assert_eq!(parent("Inventory.kt", "stdDev", Method), Some("Inventory"));
    // Local `val`s inside a class's methods become fields of the class.
    assert_eq!(
        parent("Pricing.kt", "subtotal", Field),
        Some("PricingEngine")
    );
    let expires: Vec<_> = c
        .symbols_named("expiresAt")
        .into_iter()
        .map(|(_, s)| s.parent.as_deref().unwrap())
        .collect();
    assert_eq!(
        expires,
        ["Reservation", "Inventory"],
        "the second is a local in reserveLine"
    );
    // Companion objects and `typealias` leave no symbol.
    for name in ["Factory", "Rate", "Handler"] {
        assert!(c.symbols_named(name).is_empty(), "{name} became a symbol");
    }
    // Infix and operator calls (`a percentOf b`, `a + b`) are no calls.
    assert!(!c.has_relation("vat", Calls, "percentOf"));
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    let callers = index.find_callers("sum").unwrap();
    let mut files: Vec<_> = callers.iter().map(|h| h.relative_path.as_str()).collect();
    files.sort();
    files.dedup();
    assert!(files.contains(&"order/Order.kt"), "{files:?}");
    assert!(files.contains(&"pricing/Pricing.kt"), "{files:?}");
    let calls = index.find_calls("place").unwrap();
    for callee in ["priceLines", "quote", "convert", "reserveOrReject"] {
        assert!(
            calls.iter().any(|h| h.to_name == callee),
            "place should call {callee}"
        );
    }
}
