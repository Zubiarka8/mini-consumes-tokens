//! A type named in a field, parameter, return, local, generic argument,
//! cast, `instanceof` or class literal is a `references` relation from where
//! it is written, so a class or interface used only as a type is not dead
//! code. A type parameter, the type of `new T()` (already a call), a
//! supertype (already an `extends`/`implements`) and a declared name record
//! nothing more.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_java::JavaParser;

const SOURCE: &str = r#"import java.util.List;

class A<T> extends Base implements Handler {
    private Config config;
    T value;
    List<Row> rows;
    Map<Key, Integer> index;

    Out make(In in) throws Failure {
        Local l = new Maker();
        Class<?> c = Kind.class;
        return (Out) in;
    }

    boolean is(Object o) { return o instanceof Marker; }

    <U> U generic(U u) { return u; }
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JavaParser, "A.java", "Java", SOURCE)
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
        "Config", "Row", "Key", "Out", "In", "Failure", "Local", "Kind", "Marker",
    ] {
        assert!(
            refs.iter().any(|r| r == name),
            "{name} missing from {refs:?}"
        );
    }
}

#[test]
fn a_type_parameter_a_creation_and_a_supertype_add_no_reference() {
    let refs = references(&parsed());
    for name in ["T", "U", "Maker", "Base", "Handler"] {
        assert!(!refs.iter().any(|r| r == name), "{name} in {refs:?}");
    }
}

#[test]
fn a_jdk_import_is_external_not_a_repository_candidate() {
    let file = parsed();
    let index = file
        .relations
        .iter()
        .position(|r| r.kind == RelationKind::References && r.to_name == "List")
        .expect("List is referenced");
    assert!(file
        .relation_targets
        .iter()
        .any(|t| t.relation == index && t.external));
}
