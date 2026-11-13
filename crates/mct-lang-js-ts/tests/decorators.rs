//! A decorator is a function the runtime calls with what it decorates, so
//! `@sealed` is `calls sealed`, `@ns.track` is `calls track` (a member
//! call), and `@Component({…})` is the call it spells, as in an
//! expression. The call comes from the decorated symbol: a class (its own
//! decorators and those of the `export` that declares it), a field or a
//! method. A parameter's decorator is a call from its method. TypeScript
//! puts a method's decorators before it in the class body, JavaScript
//! inside it; both are the method's.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_js_ts::JsTsParser;

const TS: &str = r#"import { Component, Input } from "@angular/core";
function sealed(t) {}
@sealed
@Component({ selector: "x" })
@ns.track()
export class Cart {
  @Input() size = 0;
  @log
  add(@Inject(TOKEN) x) {}
  @bound handle = () => {};
}
@sealed
class Plain {}
"#;

const JS: &str = r#"@tracked
class Basket {
  @bound go() {}
}
"#;

/// `from -> to` for every call, in source order.
fn calls(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| {
            let from = parsed.symbols.iter().find(|s| s.id == r.from).unwrap();
            format!("{} -> {}", from.name, r.to_name)
        })
        .collect()
}

#[test]
fn a_decorator_is_a_call_from_what_it_decorates() {
    let parsed = mct_corpus::parse_source(&JsTsParser, "src/cart.ts", "TypeScript", TS);
    let mut calls = calls(&parsed);
    calls.sort();
    assert_eq!(
        calls,
        [
            "Cart -> Component",
            "Cart -> sealed",
            "Cart -> track",
            "Plain -> sealed",
            "add -> Inject",
            "add -> log",
            "handle -> bound",
            "size -> Input",
        ]
    );
}

#[test]
fn a_javascript_method_decorator_is_a_call_from_the_method() {
    let parsed = mct_corpus::parse_source(&JsTsParser, "src/basket.js", "JavaScript", JS);
    let mut calls = calls(&parsed);
    calls.sort();
    assert_eq!(calls, ["Basket -> tracked", "go -> bound"]);
}
