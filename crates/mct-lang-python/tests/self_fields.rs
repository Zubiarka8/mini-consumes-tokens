//! `self.x = …` in a method declares `x` as a `field` of the class, once,
//! unless the class already declares `x` (a class attribute or a method).
//! Only the method's own receiver counts: its first parameter, any name,
//! not rebound, and not in a `@staticmethod`. A nested function, a lambda,
//! another object's attribute (`other.x = …`), a subscript and an
//! augmented assignment declare nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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
