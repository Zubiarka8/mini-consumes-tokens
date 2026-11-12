//! A name a module-level assignment binds is a `variable` of the module, or
//! a `constant` when annotated `Final`; one a class body binds (a class
//! attribute, a dataclass field) is a `field` of the class. Each name of
//! `a, b = …` and of a chained `x = y = …` counts, once, and the signature
//! is the target as written with its annotation. A name the file already
//! declares (`f = wrap(f)`), `_`, an attribute or subscript target, an
//! augmented assignment, and a local inside a function declare nothing
//! (`self.x` in a method is a field: `tests/self_fields.rs`). Python has no
//! visibility syntax.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, SymbolKind};
use mct_lang_python::PythonParser;

const SOURCE: &str = r#"from typing import Final

MAX_ITEMS: Final = 10
cache = {}
first, second = 1, 2
left = right = None
timeout: float
cache = {"again": 1}
_ = ignored()
cache["k"] = 1
cache += {}

if DEBUG:
    verbose = True


def run():
    local = 1
    return local


run = wrap(run)


class Cart:
    currency = "EUR"
    items: list[str] = []

    def add(self, item):
        count = 1
        self.total = count


@dataclass
class Item:
    sku: str
    price: int = 0
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&PythonParser, "shop/cart.py", "Python", SOURCE)
}

#[test]
fn module_and_class_bindings_are_symbols() {
    let parsed = parsed();
    let rows: Vec<String> = parsed
        .symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Field | SymbolKind::Constant | SymbolKind::Variable
            )
        })
        .map(|s| {
            let signature = parsed
                .symbol_details
                .iter()
                .find(|d| d.symbol == s.id)
                .and_then(|d| d.signature.clone())
                .unwrap_or_default();
            format!(
                "{} {:?} {} {} `{signature}`",
                s.location.line,
                s.kind,
                s.name,
                s.parent.as_deref().unwrap_or("-"),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            "3 Constant MAX_ITEMS - `MAX_ITEMS: Final`",
            "4 Variable cache - `cache`",
            "5 Variable first - `first`",
            "5 Variable second - `second`",
            "6 Variable left - `left`",
            "6 Variable right - `right`",
            "7 Variable timeout - `timeout: float`",
            "14 Variable verbose - `verbose`",
            "26 Field currency Cart `currency`",
            "27 Field items Cart `items: list[str]`",
            "31 Field total Cart `total`",
            "36 Field sku Item `sku: str`",
            "37 Field price Item `price: int`",
        ]
    );
}
