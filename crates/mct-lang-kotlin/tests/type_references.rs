//! A type named in a property, parameter, return type, local, generic
//! argument, cast or `is` check is a `references` relation from where it is
//! written, so a class or interface used only as a type is not dead code. A
//! type parameter, a supertype (already `extends`/`implements`) and an
//! annotation (already a reference of its own) add nothing more.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_kotlin::KotlinParser;

const SOURCE: &str = r#"class A<T> : Base(), Handler {
    private val config: Config? = null
    val rows: List<Row> = listOf()
    val index: Map<Key, Int> = mapOf()
    var value: T? = null

    fun make(input: In): Out? = null
    fun check(o: Any) = o is Marker
    fun cast(o: Any) = o as Flag
    fun <U> generic(u: U): U = u
}

fun Money.twice(): Money = this

@Audited
class B
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/A.kt", "Kotlin", SOURCE)
}

fn references(file: &ParsedFile) -> Vec<String> {
    file.relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| r.to_name.clone())
        .collect()
}

#[test]
fn a_type_in_a_declaration_or_expression_is_referenced() {
    let refs = references(&parsed());
    for name in [
        "Config", "Row", "Key", "In", "Out", "Marker", "Flag", "Money",
    ] {
        assert!(
            refs.iter().any(|r| r == name),
            "{name} missing from {refs:?}"
        );
    }
}

#[test]
fn a_type_parameter_a_supertype_and_an_annotation_add_no_type_reference() {
    let refs = references(&parsed());
    for name in ["T", "U", "Base", "Handler"] {
        assert!(!refs.iter().any(|r| r == name), "{name} in {refs:?}");
    }
    assert_eq!(
        refs.iter().filter(|r| *r == "Audited").count(),
        1,
        "{refs:?}"
    );
}
