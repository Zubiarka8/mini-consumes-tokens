//! An annotation names an annotation type (JLS §9.7): `@Marker` on a
//! declaration is a use of the type `Marker`, the way a C# attribute is a
//! use of its attribute class.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_java::JavaParser;

fn parse(src: &str) -> ParsedFile {
    mct_corpus::parse_source(&JavaParser, "src/shop/Cart.java", "Java", src)
}

/// `(from, to, line, target kind)` of every `references` relation.
fn references(parsed: &ParsedFile) -> Vec<(&str, &str, u32, Option<SymbolKind>)> {
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
                .map_or("", |s| s.name.as_str());
            let kind = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.kind);
            (from, r.to_name.as_str(), r.location.line, kind)
        })
        .collect()
}

#[test]
fn an_annotation_references_its_annotation_type() {
    let parsed = parse(
        "package shop;\n\n@Audited\npublic class Cart {\n    @Inject(order = 1) private Store store, backup;\n\n    @shop.meta.Audited @Override\n    public String toString() { return \"\"; }\n\n    @Audited Cart() {}\n}\n",
    );
    let interface = Some(SymbolKind::Interface);
    assert_eq!(
        references(&parsed),
        [
            ("Cart", "Audited", 3, interface),
            ("store", "Inject", 5, interface),
            ("backup", "Inject", 5, interface),
            ("toString", "Audited", 7, interface),
            ("toString", "Override", 7, interface),
            ("Cart", "Audited", 10, interface),
        ]
    );
}
