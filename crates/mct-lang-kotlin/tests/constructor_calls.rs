//! A superclass constructor call calls that class, as `Base(…)` in an
//! expression does: `: Base(4)` in a class header, and `super(…)` in a
//! secondary constructor of a class whose header names one supertype
//! without `()`. A Kotlin constructor is no symbol, so the call comes from
//! the class. `this(…)` would be a call from the class to itself, so it
//! gives none; nor does `super(…)` when the header names several
//! supertypes, since the parser can't tell which is the class.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_kotlin::KotlinParser;

const SOURCE: &str = r#"package shop

class Cart(n: Int) : store.Basket<Item>(n), Sized {
    constructor() : this(1)
}

class Items : Basket {
    constructor(n: Int) : super(n)
}

class Mixed : Basket, Sized {
    constructor(n: Int) : super(n)
}

class Plain {
    constructor() : super()
}

object Empty : Basket(0)
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/shop/Cart.kt", "Kotlin", SOURCE)
}

/// `from -> to` (plus ` class` when the target must be a class, and
/// ` external`) for every call whose source is a class.
fn class_calls(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .filter_map(|(i, r)| {
            let from = parsed
                .symbols
                .iter()
                .find(|s| s.id == r.from && s.kind == SymbolKind::Class)?;
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            let class = target.is_some_and(|t| t.kind == Some(SymbolKind::Class));
            let external = target.is_some_and(|t| t.external);
            Some(format!(
                "{} -> {}{}{}",
                from.name,
                r.to_name,
                if class { " class" } else { "" },
                if external { " external" } else { "" }
            ))
        })
        .collect()
}

#[test]
fn a_superclass_constructor_call_calls_the_superclass() {
    assert_eq!(
        class_calls(&parsed()),
        [
            "Cart -> Basket class",
            "Items -> Basket class",
            "Plain -> Any external",
            "Empty -> Basket class",
        ]
    );
}

#[test]
fn the_call_is_at_the_superclass_name_or_the_super_keyword() {
    let parsed = parsed();
    let lines: Vec<(u32, u32)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| (r.location.line, r.location.column))
        .collect();
    assert_eq!(lines, [(3, 28), (8, 27), (16, 21), (19, 16)]);
}
