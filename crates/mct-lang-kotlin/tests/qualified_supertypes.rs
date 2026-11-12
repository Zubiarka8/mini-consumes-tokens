//! A supertype written with its package (`store.Basket<Item>`) names the
//! class `Basket`, the last segment, exactly as `Basket<Item>` does: the
//! `extends`/`implements` relation and the supertype an `override` member
//! overrides both take that segment, never the package `store`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_kotlin::KotlinParser;

const SOURCE: &str = r#"package shop

class Cart : store.Basket<Item>(), store.Sized {
    override fun size(): Int = 0
}

interface Bag : store.Sized
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/shop/Cart.kt", "Kotlin", SOURCE)
}

fn names_of(parsed: &ParsedFile, kind: RelationKind) -> Vec<String> {
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| r.to_name.clone())
        .collect()
}

#[test]
fn a_qualified_supertype_names_its_last_segment() {
    let parsed = parsed();
    assert_eq!(
        names_of(&parsed, RelationKind::Extends),
        ["Basket", "Sized"]
    );
    assert_eq!(names_of(&parsed, RelationKind::Implements), ["Sized"]);
}

#[test]
fn an_override_targets_the_qualified_supertypes_last_segment() {
    let parsed = parsed();
    let overrides: Vec<_> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Overrides)
        .flat_map(|(i, _)| {
            parsed
                .relation_targets
                .iter()
                .filter(move |t| t.relation == i)
                .map(|t| t.qualifier.clone().unwrap_or_default())
        })
        .collect();
    assert!(overrides.iter().all(|q| q != "store"), "{overrides:?}");
    assert!(overrides.iter().any(|q| q == "Basket"), "{overrides:?}");
}
