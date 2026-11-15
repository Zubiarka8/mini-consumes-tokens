//! A callable reference (`::parse`, `Box::size`) names a function without
//! calling it, so it is a `references` relation to that function from where
//! it is written: a function used only that way is not dead code. A
//! `Type::name` reference is qualified by `Type`; a class reference
//! (`Box::class`) is no function.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_kotlin::KotlinParser;

const SOURCE: &str = r#"fun parse(s: String): Int = 1
fun twice(x: Int) = x * 2
class Box { fun size() = 1 }
fun main() {
    listOf("a").map(::parse)
    val f = ::twice
    val g = Box::size
    val k = Box::class
    val s = String::length
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/refs.kt", "Kotlin", SOURCE)
}

/// `(to_name, qualifier)` of every `References` relation.
fn references(file: &ParsedFile) -> Vec<(String, Option<String>)> {
    file.relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::References)
        .map(|(i, r)| {
            let qualifier = file
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.qualifier.clone());
            (r.to_name.clone(), qualifier)
        })
        .collect()
}

#[test]
fn an_unqualified_callable_reference_references_the_function() {
    let refs = references(&parsed());
    assert!(refs.contains(&("parse".into(), None)), "{refs:?}");
    assert!(refs.contains(&("twice".into(), None)), "{refs:?}");
}

#[test]
fn a_type_qualified_reference_carries_its_type() {
    let refs = references(&parsed());
    assert!(
        refs.contains(&("size".into(), Some("Box".into()))),
        "{refs:?}"
    );
    assert!(
        refs.contains(&("length".into(), Some("String".into()))),
        "{refs:?}"
    );
}

#[test]
fn a_class_reference_is_no_function() {
    let refs = references(&parsed());
    assert!(!refs.iter().any(|(name, _)| name == "class"), "{refs:?}");
}
