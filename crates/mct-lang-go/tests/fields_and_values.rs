//! A struct's named fields are `field`s of the struct, and a package-level
//! `const` / `var` name is a `constant` / `variable` of the package, each
//! name of `a, b int` and of a `const ( … )` group too, with its declaration
//! as signature and the exported-name visibility. An embedded field names
//! no field of its own, and a `const` or `var` inside a function declares
//! nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, SymbolKind, Visibility};
use mct_lang_go::GoParser;

const SOURCE: &str = r#"package shop

const MaxItems = 10

const (
	StateNew = iota
	stateDone
)

var (
	cache    = map[string]int{}
	hits, Misses int
)

type Cart struct {
	ID    string `json:"id"`
	items []Item
	x, Y  int
	*Base
}

func run() {
	const local = 1
	var count int
	_ = count
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&GoParser, "shop/cart.go", "Go", SOURCE)
}

/// `(name, kind, parent, line, signature, visibility)` of a symbol.
type Row = (
    String,
    SymbolKind,
    Option<String>,
    u32,
    Option<String>,
    Option<Visibility>,
);

#[test]
fn fields_constants_and_variables_are_symbols() {
    let parsed = parsed();
    let rows: Vec<Row> = parsed
        .symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Field | SymbolKind::Constant | SymbolKind::Variable
            )
        })
        .map(|s| {
            let detail = parsed.symbol_details.iter().find(|d| d.symbol == s.id);
            (
                s.name.clone(),
                s.kind,
                s.parent.clone(),
                s.location.line,
                detail.and_then(|d| d.signature.clone()),
                detail.and_then(|d| d.visibility),
            )
        })
        .collect();
    let row = |name: &str, kind, parent: &str, line, signature: &str, exported: bool| {
        (
            name.to_string(),
            kind,
            Some(parent.to_string()),
            line,
            Some(signature.to_string()),
            Some(if exported {
                Visibility::Public
            } else {
                Visibility::Package
            }),
        )
    };
    use SymbolKind::{Constant, Field, Variable};
    assert_eq!(
        rows,
        [
            row("MaxItems", Constant, "shop", 3, "const MaxItems", true),
            row("StateNew", Constant, "shop", 6, "const StateNew", true),
            row("stateDone", Constant, "shop", 7, "const stateDone", false),
            row("cache", Variable, "shop", 11, "var cache", false),
            row("hits", Variable, "shop", 12, "var hits int", false),
            row("Misses", Variable, "shop", 12, "var Misses int", true),
            row("ID", Field, "Cart", 16, "ID string", true),
            row("items", Field, "Cart", 17, "items []Item", false),
            row("x", Field, "Cart", 18, "x int", false),
            row("Y", Field, "Cart", 18, "Y int", true),
        ]
    );
}
