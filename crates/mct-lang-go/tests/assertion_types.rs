//! A type named by an assertion (`o.(Marker)`) or a type-switch case
//! (`case Row, *Cfg:`) is a use of it: a `references` relation with the
//! evidence a declared type gets. A predeclared type (`int`, `error`) gives
//! none.

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::RelationKind;
use mct_lang_go::GoParser;

const SOURCE: &str = r#"package shop

type Marker struct{}
type Row struct{}
type Cfg struct{}

func f(o any) {
	_, _ = o.(Marker)
	_ = o.(int)
	switch v := o.(type) {
	case Row, *Cfg:
		_ = v
	case error:
	case nil:
	}
}
"#;

#[test]
fn an_asserted_type_is_referenced() {
    let parsed = mct_corpus::parse_source(&GoParser, "shop/a.go", "Go", SOURCE);
    let refs: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| (r.to_name.as_str(), r.location.line))
        .collect();
    assert_eq!(refs, [("Marker", 8), ("Row", 11), ("Cfg", 11)]);
}
