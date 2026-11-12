//! `self.x = …` in a method declares `x` as a `field` of the class, once,
//! unless the class already declares `x` (a class attribute or a method).
//! Only the method's own receiver counts: its first parameter, any name,
//! not rebound, and not in a `@staticmethod`. A nested function, a lambda,
//! another object's attribute (`other.x = …`), a subscript and an
//! augmented assignment declare nothing.
//!
//! A field is a call target only through `self` (`self.key(row)`, a
//! callable attribute): `CONFIG.items()` on a receiver of unproven type
//! rules a field out, so it can't resolve to `Page.items` by name alone.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, SymbolKind};
use mct_lang_python::PythonParser;

const SOURCE: &str = r#"class Cart:
    currency = "EUR"

    def __init__(this, owner):
        this.owner = owner
        this.items: list[str] = []
        this.count, this.total = 0, 0
        this.currency = "USD"
        this.owner = None
        this.cache["k"] = 1
        this.count += 1
        owner.cart = this

        def helper():
            this.hidden = 1

        call(lambda: setattr(this, "x", 1))

    def reset(self):
        self.discount = 0
        self.size = 0

    def size(self):
        return 0

    @staticmethod
    def build(cart):
        cart.built = True

    def rebind(self):
        self = other()
        self.lost = 1


class Order:
    def __init__(self):
        self.owner = None
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&PythonParser, "shop/cart.py", "Python", SOURCE)
}

#[test]
fn a_self_attribute_is_a_field_of_the_class() {
    let parsed = parsed();
    let rows: Vec<String> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Field)
        .map(|s| {
            let signature = parsed
                .symbol_details
                .iter()
                .find(|d| d.symbol == s.id)
                .and_then(|d| d.signature.clone())
                .unwrap_or_default();
            format!(
                "{} {} {} `{signature}`",
                s.location.line,
                s.name,
                s.parent.as_deref().unwrap_or("-"),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            "2 currency Cart `currency`",
            "5 owner Cart `owner`",
            "6 items Cart `items: list[str]`",
            "7 count Cart `count`",
            "7 total Cart `total`",
            "20 discount Cart `discount`",
            "37 owner Order `owner`",
        ]
    );
}

/// A class with a `self.items` field, and calls named `items` that can or
/// can't reach it.
const PAGE: &str = r#"class Page:
    def __init__(self, rows, key):
        self.items = rows
        self.key = key

    def first(self, row):
        return self.key(row)
"#;

const REPORT: &str = r#"from shop.config import CONFIG


def report(rows):
    for name, value in CONFIG.items():
        rows.append(name)
"#;

#[test]
fn a_call_on_an_unproven_receiver_never_resolves_to_a_field() {
    let root = std::env::temp_dir().join(format!("mct-py-self-fields-{}", std::process::id()));
    std::fs::create_dir_all(root.join("shop")).unwrap();
    std::fs::write(root.join("shop/page.py"), PAGE).unwrap();
    std::fs::write(root.join("shop/report.py"), REPORT).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PythonParser));
    let mut hits: Vec<String> = ["items", "key"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| {
            format!(
                "{} {}:{} {:?}",
                h.to_name, h.relative_path, h.line, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "items shop/report.py:5 Unresolved",
            "key shop/page.py:7 Resolved",
        ]
    );
}

#[test]
fn a_decorator_on_an_unproven_receiver_rules_out_a_field() {
    const ADMIN: &str = r#"from django.contrib import admin


class BookAdmin:
    @admin.action(description="Mark lost")
    def mark_lost(self, request):
        pass

    @cached
    def total(self):
        pass
"#;
    let parsed = mct_corpus::parse_source(&PythonParser, "shop/admin.py", "Python", ADMIN);
    let rows: Vec<String> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == mct_core::RelationKind::References)
        .map(|(i, r)| {
            let not_field = parsed
                .relation_targets
                .iter()
                .any(|t| t.relation == i && t.not_field);
            format!("{} {} {not_field}", r.location.line, r.to_name)
        })
        .collect();
    assert_eq!(rows, ["5 action true", "9 cached false"]);
}
