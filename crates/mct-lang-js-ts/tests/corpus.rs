//! Long, multi-file fixture corpus (issue #74): the back office of a
//! warehouse in TypeScript, TSX, JSX and CommonJS — money, the order
//! aggregate, inventory and pick lists, a typed HTTP client, a React order
//! table, a plain-JavaScript picking board and a Node seeding script — each
//! file 300–600 lines and importing the others.
//! The shared checks (size, line ranges, golden snapshot, index round trip,
//! malformed input) come from `mct-corpus`; the tests below pin what the
//! JS/TS parser is expected to extract and its documented limits.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-js-ts/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_js_ts::JsTsParser;

mct_corpus::standard_tests!(JsTsParser);

use RelationKind::{Calls, Extends, Implements, Imports, References};
use SymbolKind::{Class, Field, Function, Interface, Method, TypeAlias};

fn parent<'a>(path: &str, name: &str, kind: SymbolKind) -> Option<&'a str> {
    corpus().symbol(path, name, kind).parent.as_deref()
}

#[test]
fn declarations_of_every_kind_are_extracted() {
    assert_eq!(parent("money.ts", "Money", Class), None);
    assert_eq!(parent("money.ts", "Comparable", Interface), None);
    assert_eq!(parent("money.ts", "MoneyJson", TypeAlias), None);
    assert_eq!(parent("money.ts", "roundHalfEven", Function), None);
    assert_eq!(parent("order.ts", "OrderEvent", TypeAlias), None);
    assert_eq!(parent("order.ts", "Placed", Interface), None);
    assert_eq!(parent("inventory.ts", "Observable", Class), None);
    assert_eq!(parent("client.ts", "WarehouseClient", Class), None);
    // Generator functions and arrow functions bound to a `const`.
    assert_eq!(parent("order.ts", "pages", Function), None);
    assert_eq!(parent("money.ts", "sum", Function), None);
    assert_eq!(parent("OrderTable.tsx", "StatusPill", Function), None);
    assert_eq!(parent("seed.cjs", "parseArgs", Function), None);
}

#[test]
fn class_members_attach_to_their_class() {
    assert_eq!(parent("money.ts", "plus", Method), Some("Money"));
    assert_eq!(parent("money.ts", "ofMinor", Method), Some("Money"));
    // `#private` members keep their `#`.
    assert_eq!(
        parent("money.ts", "#requireSameCurrency", Method),
        Some("Money")
    );
    assert_eq!(parent("money.ts", "#minor", Field), Some("Money"));
    assert_eq!(parent("order.ts", "#record", Method), Some("Order"));
    // Getters are methods.
    assert_eq!(parent("money.ts", "isZero", Method), Some("Money"));
    // Async and async-generator methods.
    assert_eq!(
        parent("client.ts", "placeOrder", Method),
        Some("WarehouseClient")
    );
    assert_eq!(
        parent("client.ts", "orders", Method),
        Some("WarehouseClient")
    );
    assert_eq!(parent("inventory.ts", "#store", Method), Some("StockCache"));
    assert_eq!(parent("order.ts", "trackingNumber", Field), Some("Order"));
}

#[test]
fn heritage_clauses_are_extends_and_implements() {
    let c = corpus();
    c.relation("money.ts", "CurrencyMismatchError", Extends, "Error");
    c.relation("money.ts", "Money", Implements, "Comparable");
    c.relation("inventory.ts", "StockCache", Extends, "Observable");
    c.relation("client.ts", "WarehouseClient", Implements, "ExchangeRates");
    // Interface `extends`.
    c.relation("order.ts", "Placed", Extends, "EventBase");
}

#[test]
fn imports_record_the_local_names_and_required_modules() {
    let c = corpus();
    // Named, type-only, default, namespace and aliased imports.
    c.relation("order.ts", "order", Imports, "sum");
    c.relation("order.ts", "order", Imports, "MoneyJson");
    c.relation("order.ts", "order", Imports, "clock");
    c.relation("client.ts", "client", Imports, "Order");
    c.relation("client.ts", "client", Imports, "describeEvent");
    c.relation("OrderTable.tsx", "OrderTable", Imports, "useReducer");
    // CommonJS `require(…)`.
    c.relation("seed.cjs", "seed", Imports, "node:fs");
    c.relation("seed.cjs", "seed", Imports, "../package.json");
}

#[test]
fn exports_by_name_are_references() {
    let c = corpus();
    c.relation("order.ts", "order", References, "Order");
    c.relation("client.ts", "client", References, "asOrderId");
    c.relation(
        "OrderTable.tsx",
        "OrderTable",
        References,
        "validatePlaceOrder",
    );
}

#[test]
fn calls_through_every_expression_shape_are_extracted() {
    let c = corpus();
    // Plain, member, private-member, optional and static calls.
    c.relation("money.ts", "plus", Calls, "#requireSameCurrency");
    c.relation("money.ts", "of", Calls, "round");
    c.relation("client.ts", "#request", Calls, "onRequest");
    c.relation("order.ts", "place", Calls, "randomUUID");
    // Calls inside callbacks belong to the enclosing function.
    c.relation("money.ts", "sum", Calls, "plus");
    c.relation("OrderTable.tsx", "useOrders", Calls, "dispatch");
    // `new X(…)` is a call to X.
    c.relation("money.ts", "parse", Calls, "MoneyFormatError");
}

#[test]
fn cross_file_calls_are_extracted() {
    let c = corpus();
    c.relation("order.ts", "fromJSON", Calls, "fromJSON");
    c.relation("client.ts", "order", Calls, "fromJSON");
    c.relation("inventory.ts", "postCounts", Calls, "invalidate");
    c.relation("OrderTable.tsx", "applyFilters", Calls, "isLarge");
    c.relation(
        "OrderTable.tsx",
        "createOrderTable",
        Calls,
        "httpStockSource",
    );
    assert!(c.cross_file_relation_count() >= 60);
}

#[test]
fn documented_limits_of_the_parser() {
    let c = corpus();
    // Known limit: a namespace's functions are top-level functions.
    assert_eq!(parent("money.ts", "fromCents", Function), None);
    // Known limit: interface members are not symbols.
    assert!(c
        .symbols_named("compareTo")
        .iter()
        .all(|(_, s)| s.parent.as_deref() == Some("Money")));
    // Known limit: JSX elements are not calls of their component (see the
    // crate docs).
    assert!(!c.has_relation("OrderTable", Calls, "StockBadge"));
}

#[test]
fn ts_enums_and_namespaces_are_symbols() {
    assert_eq!(parent("money.ts", "Currency", SymbolKind::Enum), None);
    assert_eq!(parent("order.ts", "State", SymbolKind::Enum), None);
    assert_eq!(parent("money.ts", "Legacy", SymbolKind::Module), None);
}

#[test]
fn plain_javascript_class_fields_and_jsx_use_the_js_grammar() {
    let c = corpus();
    // `.jsx` goes through tree-sitter-javascript: `field_definition`, not
    // TS's `public_field_definition`. An arrow-valued field is a method…
    assert_eq!(
        parent("PickBoard.jsx", "handleScan", Method),
        Some("PickSession")
    );
    assert_eq!(
        parent("PickBoard.jsx", "retry", Method),
        Some("ScanErrorBoundary")
    );
    // …any other value a field, `static` and `#private` included.
    assert_eq!(
        parent("PickBoard.jsx", "state", Field),
        Some("ScanErrorBoundary")
    );
    assert_eq!(
        parent("PickBoard.jsx", "#nextId", Field),
        Some("PickSession")
    );
    assert_eq!(
        parent("PickBoard.jsx", "getDerivedStateFromError", Method),
        Some("ScanErrorBoundary")
    );
    c.relation("PickBoard.jsx", "ScanErrorBoundary", Extends, "Component");
    c.relation("PickBoard.jsx", "handleScan", Calls, "formatLocation");
    // A namespace import records its local name.
    c.relation("PickBoard.jsx", "PickBoard", Imports, "orders");
    // JSX elements are not calls here either.
    assert!(!c.has_relation("Board", Calls, "TaskCard"));
}

#[test]
fn object_literal_methods_and_nested_functions_are_parentless_functions() {
    // An object literal is not a type: its methods (getters included) are
    // functions, as in a CommonJS `module.exports = { … }`.
    assert_eq!(parent("inventory.ts", "levels", Function), None);
    assert_eq!(parent("money.ts", "rate", Function), None);
    assert_eq!(parent("seed.cjs", "sent", Function), None);
    // Calls inside them stay theirs.
    corpus().relation("inventory.ts", "levels", Calls, "parseLocation");
    // Nested functions are functions too, never members of the enclosing
    // class or function.
    assert_eq!(parent("order.ts", "waitedMinutes", Function), None);
    assert_eq!(parent("OrderTable.tsx", "showSlip", Function), None);
    let c = corpus();
    assert!(c
        .files
        .iter()
        .flat_map(|f| &f.parsed.symbols)
        .all(|s| s.kind != Method || s.parent.is_some()));
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    let callers = index.find_callers("fromJSON").unwrap();
    let mut files: Vec<_> = callers.iter().map(|h| h.relative_path.as_str()).collect();
    files.sort();
    files.dedup();
    assert_eq!(files, ["src/api/client.ts", "src/order.ts"]);
    let refs = index.find_references("WarehouseClient").unwrap();
    assert!(refs
        .iter()
        .any(|r| r.relative_path == "src/ui/OrderTable.tsx"));
}
