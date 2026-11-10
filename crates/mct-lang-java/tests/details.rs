//! Declaration details (ADR-034, issue #321) the Java parser proves from
//! syntax: the header without annotations or body, parameters (the explicit
//! `this` receiver, varargs), the return type, generics, visibility from the
//! access keyword (package-private without one, public in an interface) and
//! modifiers, `@Override` included.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{Modifier, ParsedFile, SymbolDetail, SymbolKind, Visibility};
use mct_lang_java::JavaParser;

const SOURCE: &str = r#"
@Entity
public abstract class Repo<T extends Item> extends Base implements Api {
    @Deprecated private static final int LIMIT = 3, MAX = 9;
    @Override
    protected synchronized <K> List<K> load(Repo<T> this, final int id,
            String... names) throws IOException {
        return null;
    }
    Repo(int x) {}
}
interface Api { void run(int[] a); }
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JavaParser, "src/Repo.java", "Java", SOURCE)
}

fn detail(parsed: &ParsedFile, name: &str, line: u32) -> SymbolDetail {
    let symbol = parsed
        .symbols
        .iter()
        // The file's own module symbol is named after it (`Repo`) too.
        .find(|s| s.name == name && s.location.line == line && s.kind != SymbolKind::Module)
        .unwrap_or_else(|| panic!("no {name} on line {line}"));
    parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == symbol.id)
        .cloned()
        .unwrap_or_else(|| panic!("no detail for {name}"))
        .normalized()
}

/// `name:type:kind` per parameter.
fn params(detail: &SymbolDetail) -> Vec<String> {
    detail
        .parameters
        .iter()
        .map(|p| {
            format!(
                "{}:{}:{}",
                p.name.as_deref().unwrap_or("_"),
                p.type_text.as_ref().map_or("?", |t| t.text.as_str()),
                p.kind.as_str()
            )
        })
        .collect()
}

#[test]
fn a_method_header_drops_annotations_and_keeps_throws() {
    let parsed = parsed();
    let load = detail(&parsed, "load", 5);
    assert_eq!(
        load.signature.as_deref(),
        Some(
            "protected synchronized <K> List<K> load(Repo<T> this, final int id, \
             String... names) throws IOException"
        )
    );
    assert_eq!(load.visibility, Some(Visibility::Protected));
    assert_eq!(load.modifiers, [Modifier::Override]);
    assert_eq!(load.generics.as_deref(), Some("<K>"));
    assert_eq!(
        load.return_type.as_ref().map(|t| t.text.as_str()),
        Some("List<K>")
    );
    assert_eq!(
        params(&load),
        [
            "this:Repo<T>:receiver",
            "id:int:positional",
            "names:String:variadic"
        ]
    );
}

#[test]
fn a_class_and_each_field_of_a_declaration_get_their_header() {
    let parsed = parsed();
    let repo = detail(&parsed, "Repo", 2);
    assert_eq!(
        repo.signature.as_deref(),
        Some("public abstract class Repo<T extends Item> extends Base implements Api")
    );
    assert_eq!(repo.visibility, Some(Visibility::Public));
    assert_eq!(repo.modifiers, [Modifier::Abstract]);
    assert_eq!(repo.generics.as_deref(), Some("<T extends Item>"));
    for (name, signature) in [
        ("LIMIT", "private static final int LIMIT"),
        ("MAX", "private static final int MAX"),
    ] {
        let field = detail(&parsed, name, 4);
        assert_eq!(field.signature.as_deref(), Some(signature));
        assert_eq!(field.visibility, Some(Visibility::Private));
        assert_eq!(field.modifiers, [Modifier::Final, Modifier::Static]);
    }
}

#[test]
fn without_an_access_keyword_a_member_is_package_private_except_in_an_interface() {
    let parsed = parsed();
    let constructor = detail(&parsed, "Repo", 10);
    assert_eq!(constructor.signature.as_deref(), Some("Repo(int x)"));
    assert_eq!(constructor.visibility, Some(Visibility::Package));
    assert!(constructor.return_type.is_none());
    let api = detail(&parsed, "Api", 12);
    assert_eq!(api.visibility, Some(Visibility::Package));
    let run = detail(&parsed, "run", 12);
    assert_eq!(run.signature.as_deref(), Some("void run(int[] a)"));
    assert_eq!(run.visibility, Some(Visibility::Public));
    assert_eq!(params(&run), ["a:int[]:positional"]);
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-java-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Repo.java"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JavaParser));
    let load = index
        .symbol_details("src/Repo.java", 5, "load")
        .unwrap()
        .unwrap();
    assert_eq!(load.visibility.as_deref(), Some("protected"));
    assert_eq!(load.parameters.len(), 3);
    std::fs::remove_dir_all(root).unwrap();
}
