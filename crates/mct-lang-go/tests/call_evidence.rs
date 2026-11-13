//! What a call's syntax proves about its target. Go has no implicit
//! receiver, so a bare `f()` names a package-level declaration of this
//! file's package, unless `f` is a local. `v.f()` on a value (a declared
//! name, an index, a call result, a field) whose type the syntax doesn't
//! prove calls a method, never a package-level name. `pkg.F()` through an
//! unaliased import whose path ends in `pkg` names a declaration of that
//! package. Anything else (an aliased import, an undeclared name) proves
//! nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_go::GoParser;

const SOURCE: &str = r#"package shop

import (
	db "example.com/app/database"
	"example.com/app/store"
)

func run(items []Item, c Cart) {
	helper()
	store.Open()
	db.Open()
	items[0].Price()
	build().Total()
	c.Total()
	x := build()
	x.Total()
	unknown.Total()
	f := func() {}
	f()
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&GoParser, "shop/run.go", "Go", SOURCE)
}

/// `line name evidence` for every call.
fn calls(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .map(|(i, r)| {
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            let evidence = match target {
                Some(t) if t.kind == Some(SymbolKind::Method) => "method".to_string(),
                Some(t) => t.qualifier.clone().unwrap_or_else(|| "?".to_string()),
                None => "-".to_string(),
            };
            format!("{} {} {evidence}", r.location.line, r.to_name)
        })
        .collect()
}

#[test]
fn a_call_carries_what_its_syntax_proves() {
    assert_eq!(
        calls(&parsed()),
        [
            "9 helper shop",
            "10 Open store",
            "11 Open -",
            "12 Price method",
            "13 Total method",
            "13 build shop",
            "14 Total Cart",
            "15 build shop",
            "16 Total method",
            "17 Total -",
            "19 f -",
        ]
    );
}

/// An import path names a package, never a same-named field or variable
/// (`import "log"` beside a `log Logger` field).
#[test]
fn an_import_names_a_module() {
    let parsed = parsed();
    let kinds: Vec<(String, Option<SymbolKind>)> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Imports)
        .map(|(i, r)| {
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            (r.to_name.clone(), target.and_then(|t| t.kind))
        })
        .collect();
    let module = |name: &str| (name.to_string(), Some(SymbolKind::Module));
    assert_eq!(
        kinds,
        [
            module("database"),
            module("example.com/app/database"),
            module("store"),
            module("example.com/app/store"),
        ]
    );
}
