//! `this(…)` and `super(…)` in a constructor call a constructor, as
//! `new T(…)` does: `this(…)` names the enclosing class, `super(…)` the
//! class its `extends` names (the last segment of `p.Base<T>`). A class
//! with no `extends` has `Object` as its superclass, so its `super()` is
//! external, and so is a superclass bound by a single-type JDK import.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_java::JavaParser;

const SOURCE: &str = r#"package shop;

import java.util.ArrayList;

class Cart extends store.Basket<Item> {
    Cart() { this(1); }
    Cart(int n) { super(n); }
}

class Plain {
    Plain() { super(); }
}

class Items extends ArrayList<Item> {
    Items() { super(4); }
}

class Outer {
    class Inner extends Base {
        Inner(Outer o) { o.super(); }
    }
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JavaParser, "src/shop/Cart.java", "Java", SOURCE)
}

/// `Parent.from -> to` (plus ` external`) for every call from a constructor.
fn constructor_calls(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .filter_map(|(i, r)| {
            let from = parsed.symbols.iter().find(|s| s.id == r.from)?;
            let parent = from.parent.as_deref()?;
            (from.name == parent).then(|| {
                let external = parsed
                    .relation_targets
                    .iter()
                    .any(|t| t.relation == i && t.external);
                format!(
                    "{parent}.{} -> {}{}",
                    from.name,
                    r.to_name,
                    if external { " external" } else { "" }
                )
            })
        })
        .collect()
}

#[test]
fn this_and_super_call_a_constructor() {
    assert_eq!(
        constructor_calls(&parsed()),
        [
            "Cart.Cart -> Cart",
            "Cart.Cart -> Basket",
            "Plain.Plain -> Object external",
            "Items.Items -> ArrayList external",
            "Inner.Inner -> Base",
        ]
    );
}

#[test]
fn the_target_is_a_constructor_declared_in_that_class() {
    let parsed = parsed();
    let targets: Vec<_> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.kind == RelationKind::Calls && matches!(r.to_name.as_str(), "Cart" | "Basket")
        })
        .map(|(i, r)| {
            let t = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .unwrap();
            (r.to_name.as_str(), t.qualifier.as_deref(), t.kind)
        })
        .collect();
    assert_eq!(
        targets,
        [
            ("Cart", Some("Cart"), Some(mct_core::SymbolKind::Method)),
            ("Basket", Some("Basket"), Some(mct_core::SymbolKind::Method)),
        ]
    );
}
