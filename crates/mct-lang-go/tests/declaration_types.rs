//! A type named in a declaration (`var c Cart`, a parameter, a result, a
//! struct field, a local `var`), unwrapped through `*T`, `[]T`, `map[K]V`,
//! `chan T` and generic arguments, references that type with the evidence a
//! composite literal gets: a bare name is a declaration of this package,
//! `pkg.T` is external through a standard package, a declaration of `pkg`
//! through an unaliased repository import and one under the path's last
//! directory through an aliased one (`db.Conn`: under `database/`). A
//! predeclared type (`int`, `error`), a type parameter (`T`) and a type of
//! an unknown package give no relation.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_go::GoParser;

const SOURCE: &str = r#"package shop

import (
	"sync"
	db "example.com/app/database"
	"example.com/app/store"
)

var Default Cart

type Cart struct {
	Items []*Item
	mu    sync.Mutex
	conn  db.Conn
	Tags  map[string]Tag
}

type Box[T any] struct{ value T }

func Total(c *Cart, ch chan Item) (Money, error) {
	var b store.Basket
	n := 0
	_ = b
	_ = n
	return Money{}, nil
}

func Map[T any](xs []T, f func(T) Price) []Box[Item] {
	return nil
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&GoParser, "shop/cart.go", "Go", SOURCE)
}

#[test]
fn a_declared_type_references_its_type_with_package_evidence() {
    let parsed = parsed();
    let mut references: Vec<(&str, u32, String)> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::References)
        .map(|(i, r)| {
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            let evidence = match target {
                Some(t) if t.external => "external".to_string(),
                Some(t) => match (&t.qualifier, &t.module) {
                    (None, Some(directory)) => format!("under {directory}"),
                    (qualifier, _) => format!("in {}", qualifier.clone().unwrap_or_default()),
                },
                None => "unproven".to_string(),
            };
            (r.to_name.as_str(), r.location.line, evidence)
        })
        .collect();
    references.sort();
    let r = |name, line, evidence: &str| (name, line, evidence.to_string());
    assert_eq!(
        references,
        [
            r("Basket", 21, "in store"),
            r("Box", 28, "in shop"),
            r("Cart", 9, "in shop"),
            r("Cart", 20, "in shop"),
            r("Conn", 14, "under database"),
            r("Item", 12, "in shop"),
            r("Item", 20, "in shop"),
            r("Item", 28, "in shop"),
            r("Money", 20, "in shop"),
            r("Money", 25, "in shop"),
            r("Mutex", 13, "external"),
            r("Price", 28, "in shop"),
            r("Tag", 15, "in shop"),
        ]
    );
}
