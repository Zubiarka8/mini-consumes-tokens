//! An annotation names an annotation class: `@Marker` on a declaration is a
//! use of the class `Marker`, the way a C# attribute is a use of its
//! attribute class.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_kotlin::KotlinParser;

fn parse(src: &str) -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/shop/Cart.kt", "Kotlin", src)
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
fn an_annotation_references_its_annotation_class() {
    let parsed = parse(
        "package shop\n\n@Audited\nclass Cart(@field:Inject val store: Store) {\n    @Inject(order = 1) val backup: Store? = null\n\n    @shop.meta.Audited @Deprecated(\"old\")\n    fun total(): Int = 0\n}\n",
    );
    let class = Some(SymbolKind::Class);
    assert_eq!(
        references(&parsed),
        [
            ("Cart", "Audited", 3, class),
            ("store", "Inject", 4, class),
            ("backup", "Inject", 5, class),
            ("total", "Audited", 7, class),
            ("total", "Deprecated", 7, class),
        ]
    );
}
