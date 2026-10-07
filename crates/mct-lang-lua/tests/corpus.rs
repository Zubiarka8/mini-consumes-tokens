//! Long, multi-file fixture corpus (issue #74): the OpenResty API gateway of
//! a warehouse — money with metatables, the order aggregate, a stock cache,
//! token-bucket rate limiting and a router with middleware — each file
//! 300–600 lines and `require`-ing the others. The shared checks (size, line
//! ranges, golden snapshot, index round trip, malformed input) come from
//! `mct-corpus`; the tests below pin what Lua is expected to extract and the
//! parser's documented limits.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-lua/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_lua::LuaParser;

mct_corpus::standard_tests!(LuaParser);

use RelationKind::{Calls, Imports};
use SymbolKind::{Function, Method, Module};

fn parent<'a>(path: &str, name: &str, kind: SymbolKind) -> Option<&'a str> {
    corpus().symbol(path, name, kind).parent.as_deref()
}

#[test]
fn table_modules_and_their_functions_are_extracted() {
    // `local M = {}` and `local Money = {}` are modules.
    assert_eq!(parent("money.lua", "Money", Module), None);
    assert_eq!(parent("ratelimit.lua", "_M", Module), None);
    // `function M.of()` and `function Money:plus()` are methods of the table.
    assert_eq!(parent("money.lua", "of", Method), Some("M"));
    assert_eq!(parent("money.lua", "plus", Method), Some("Money"));
    assert_eq!(parent("ratelimit.lua", "take", Method), Some("Limiter"));
    assert_eq!(parent("router.lua", "dispatch", Method), Some("Router"));
    // `local function f()` is a function.
    assert_eq!(parent("money.lua", "require_same", Function), None);
    assert_eq!(parent("router.lua", "proxy", Function), None);
    // `local f = function() end` too; `T.f = function() end` is a method
    // of T, like `function T.f()`.
    assert_eq!(parent("order.lua", "subtotal", Function), None);
    assert_eq!(parent("money.lua", "__add", Method), Some("Money"));
    // A nested table field keeps its dotted owner.
    assert_eq!(parent("ratelimit.lua", "advance", Method), Some("clock"));
}

#[test]
fn require_is_an_import_of_the_module_path() {
    let c = corpus();
    c.relation("order.lua", "order", Imports, "warehouse.money");
    c.relation("order.lua", "order", Imports, "cjson.safe");
    c.relation("router.lua", "router", Imports, "gateway.ratelimit");
    c.relation("router.lua", "router", Imports, "warehouse.inventory");
    c.relation("ratelimit.lua", "ratelimit", Imports, "resty.lock");
}

#[test]
fn calls_through_every_expression_shape_are_extracted() {
    let c = corpus();
    // Plain, dotted and method (`:`) calls.
    c.relation("money.lua", "of", Calls, "round_half_even");
    c.relation("money.lua", "plus", Calls, "require_same");
    c.relation("money.lua", "percent", Calls, "times");
    c.relation("ratelimit.lua", "take", Calls, "lock");
    // Calls inside an anonymous callback belong to the enclosing function.
    c.relation("order.lua", "largest", Calls, "total");
    c.relation("inventory.lua", "start_refresher", Calls, "invalidate");
}

#[test]
fn cross_file_calls_are_extracted() {
    let c = corpus();
    c.relation("order.lua", "total", Calls, "sum");
    c.relation("order.lua", "pay", Calls, "to_json");
    c.relation("inventory.lua", "check_order", Calls, "available");
    c.relation("router.lua", "quote_preview", Calls, "sum");
    c.relation("router.lua", "handle", Calls, "access");
    assert!(c.name_matched_relation_count() >= 30);
}

#[test]
fn only_named_file_level_tables_are_modules() {
    // A local table in a function, or one stored through an index, is a
    // value, not a module.
    let c = corpus();
    for name in ["out", "params", "self.routes[#self.routes + 1]"] {
        assert!(
            c.symbols_named(name).iter().all(|(_, s)| s.kind != Module),
            "{name}"
        );
    }
}

#[test]
fn a_function_assigned_to_a_name_spans_its_body() {
    // `Money.__add = function(a, b) … end` spans through `end`, so the
    // body's calls lie inside it.
    let add = corpus().symbol("money.lua", "__add", Method);
    let call = corpus().relation("money.lua", "__add", Calls, "plus");
    let end = add.location.end_line.unwrap();
    assert!((add.location.line..=end).contains(&call.line));
    assert!(end > add.location.line);
}

#[test]
fn functions_inside_table_constructors_are_methods_of_the_table() {
    // `local handlers = { get_stock = function() … end }`.
    let c = corpus();
    for name in ["get_stock", "place_order", "get_order"] {
        assert_eq!(parent("router.lua", name, Method), Some("handlers"));
    }
    for name in ["placed", "shipped"] {
        assert_eq!(parent("order.lua", name, Method), Some("describers"));
    }
    c.relation("router.lua", "place_order", Calls, "check_order");
    c.relation("router.lua", "place_order", Calls, "from_request_body");
    // Functions added to the table afterwards are methods too.
    assert_eq!(parent("router.lua", "health", Method), Some("handlers"));
}

#[test]
fn calls_inside_the_called_expression_are_extracted() {
    // In `money.gross(net, 21):to_json()` both calls are extracted.
    let c = corpus();
    c.relation("router.lua", "quote_preview", Calls, "to_json");
    c.relation("router.lua", "quote_preview", Calls, "gross");
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    let callers = index.find_callers("of").unwrap();
    let mut files: Vec<_> = callers.iter().map(|h| h.relative_path.as_str()).collect();
    files.sort();
    files.dedup();
    assert_eq!(files, ["gateway/router.lua", "warehouse/money.lua"]);
    let refs = index.find_references("warehouse.money").unwrap();
    assert_eq!(refs.len(), 2);
}
