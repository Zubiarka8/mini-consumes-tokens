//! An embedded type is an `extends` of the struct or interface that embeds
//! it: each anonymous field of a struct (`Base`, `*Base`, `pkg.Base`,
//! `List[int]`), and each interface element naming one type (`Reader`,
//! `io.Writer`), which must be an interface. A union or `~T` element is a
//! type set, not an embedding, and gives none. A bare name is a
//! declaration of this package; `pkg.T` through a standard package is
//! external, through an unaliased repository import a declaration of
//! `pkg`, and through anything else proves nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_go::GoParser;

const SOURCE: &str = r#"package shop

import (
	"io"
	db "example.com/app/database"
	"example.com/app/store"
)

type ReadWriter interface {
	Reader
	io.Writer
	~int | string
	Close() error
}

type Cart struct {
	Base
	*Audit
	store.Basket
	db.Conn
	List[Item]
	name string
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&GoParser, "shop/cart.go", "Go", SOURCE)
}

/// `line from -> to evidence` for every `extends`.
fn embeddings(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Extends)
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
                Some(t) => format!(
                    "{}{}",
                    t.qualifier.as_deref().unwrap_or("?"),
                    t.kind.map(|k| format!(" {k:?}")).unwrap_or_default()
                ),
            };
            format!("{} {from} -> {} {evidence}", r.location.line, r.to_name)
        })
        .collect()
}

#[test]
fn an_embedded_type_is_extended() {
    assert_eq!(
        embeddings(&parsed()),
        [
            "10 ReadWriter -> Reader shop Interface",
            "11 ReadWriter -> Writer external",
            "17 Cart -> Base shop",
            "18 Cart -> Audit shop",
            "19 Cart -> Basket store",
            "20 Cart -> Conn -",
            "21 Cart -> List shop",
        ]
    );
}
