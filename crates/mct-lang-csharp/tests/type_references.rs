//! A type named in a field, property, parameter, return type, local,
//! generic argument, cast, `is`/`as` check, `typeof` or `catch` is a
//! `references` relation from the member it is written in, so a class or
//! interface used only as a type is not dead code. A type parameter, the
//! type of `new` (already a call) and a base type (already
//! `extends`/`implements`) add nothing; built-in keywords (`int`,
//! `string`) are no identifiers.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"class A<T> : Base, IHandler
{
    private Config config;
    List<Row> rows;
    T value;
    Dictionary<Key, int> Index { get; }

    Out Make(In input)
    {
        Local l = new Maker();
        var k = typeof(Kind);
        try { } catch (Failure e) { }
        return (Out) input;
    }

    bool Check(object o) => o is Marker;
    object Cast(object o) => o as Flag;
    U Generic<U>(U u) => u;
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CSharpParser, "A.cs", "C#", SOURCE)
}

fn references(file: &ParsedFile) -> Vec<(String, String)> {
    file.relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| {
            let from = file.symbols.iter().find(|s| s.id == r.from).unwrap();
            (from.name.clone(), r.to_name.clone())
        })
        .collect()
}

#[test]
fn a_type_in_a_declaration_or_expression_is_referenced() {
    let refs = references(&parsed());
    for name in [
        "Config", "Row", "Key", "Out", "In", "Local", "Kind", "Failure", "Marker", "Flag",
    ] {
        assert!(
            refs.iter().any(|(_, to)| to == name),
            "{name} missing from {refs:?}"
        );
    }
}

#[test]
fn a_reference_comes_from_the_member_that_names_the_type() {
    let refs = references(&parsed());
    assert!(refs.contains(&("Make".into(), "In".into())), "{refs:?}");
    assert!(
        refs.contains(&("Check".into(), "Marker".into())),
        "{refs:?}"
    );
    assert!(
        refs.contains(&("config".into(), "Config".into())),
        "{refs:?}"
    );
}

#[test]
fn a_type_parameter_a_creation_and_a_base_type_add_no_reference() {
    let refs = references(&parsed());
    for name in ["T", "U", "Maker", "Base", "IHandler"] {
        assert!(!refs.iter().any(|(_, to)| to == name), "{name} in {refs:?}");
    }
}
