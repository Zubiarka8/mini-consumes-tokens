//! A composite literal of a named type (`Cart{…}`, `&Cart{…}`,
//! `store.Basket{…}`, `Box[int]{…}`) references that type, with the
//! evidence an embedded type gets: a bare name is a declaration of this
//! package, `pkg.T` is external through a standard package, a
//! declaration of `pkg` through an unaliased repository import and one
//! under the path's last directory through an aliased one. A literal
//! of an unnamed type (`[]Item{…}`, `map[string]int{…}`, an elided
//! `{…}` element) references nothing; its elements are still walked.

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

var Default = Cart{}

func build() {
	c := &Cart{Items: []Item{{SKU: "a"}, Item{SKU: "b"}}}
	_ = store.Basket{}
	_ = db.Conn{}
	_ = sync.Mutex{}
	_ = Box[int]{}
	_ = map[string]int{}
	_ = c
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&GoParser, "shop/cart.go", "Go", SOURCE)
}

/// `line from -> to evidence` for every `references`.
fn references(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::References)
        .map(|(i, r)| {
            let from = parsed
                .symbols
                .iter()
                .find(|s| s.id == r.from)
                .map(|s| s.name.as_str())
                .unwrap_or_default();
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            let evidence = match target {
                None => "-".to_string(),
                Some(t) if t.external => "external".to_string(),
                Some(t) => t
                    .qualifier
                    .clone()
                    .or_else(|| t.module.as_ref().map(|m| format!("dir:{m}")))
                    .unwrap_or_else(|| "?".to_string()),
            };
            format!("{} {from} -> {} {evidence}", r.location.line, r.to_name)
        })
        .collect()
}

#[test]
fn a_composite_literal_references_its_type() {
    assert_eq!(
        references(&parsed()),
        [
            "9 cart -> Cart shop",
            "12 build -> Cart shop",
            "12 build -> Item shop",
            "13 build -> Basket store",
            "14 build -> Conn dir:database",
            "15 build -> Mutex external",
            "16 build -> Box shop",
        ]
    );
}
