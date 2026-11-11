//! Declaration details (ADR-034, issue #321) the C# parser proves from
//! syntax: the header without attributes or body, parameters (an extension
//! method's `this` parameter as the receiver, `params`, defaults), the
//! return type, generics, visibility from the access keywords (or the
//! language default for where the declaration sits) and modifiers.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{Modifier, ParsedFile, SymbolDetail, SymbolKind, Visibility};
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"
[Serializable]
public abstract partial class Repo<T> : Base, IApi where T : class {
    [Obsolete] private static readonly int Limit = 3, Max = 9;
    public string Name { get; set; } = "";
    protected internal override async Task<List<K>> LoadAsync<K>(int id = 3, params string[] names) where K : new() { return null; }
    public static string Shout(this string s, ref int n, out int m) => s;
    void IApi.Run(int a) {}
    public Repo(int x) : base(x) {}
    public int this[int i] => i;
    public event Action Changed;
    class Inner {}
}
interface IApi { void Run(int a); }
public record Point(int X, int Y);
enum Color { Red }
delegate int Op(int a);
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CSharpParser, "src/Repo.cs", "C#", SOURCE)
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

/// `name:type=default:kind` per parameter.
fn params(detail: &SymbolDetail) -> Vec<String> {
    detail
        .parameters
        .iter()
        .map(|p| {
            format!(
                "{}:{}={}:{}",
                p.name.as_deref().unwrap_or("_"),
                p.type_text.as_ref().map_or("?", |t| t.text.as_str()),
                p.default.as_deref().unwrap_or("-"),
                p.kind.as_str()
            )
        })
        .collect()
}

#[test]
fn a_method_records_defaults_params_return_type_and_modifiers() {
    let parsed = parsed();
    let load = detail(&parsed, "LoadAsync", 6);
    assert_eq!(
        load.signature.as_deref(),
        Some(
            "protected internal override async Task<List<K>> LoadAsync<K>(int id = 3, \
             params string[] names) where K : new()"
        )
    );
    // `protected internal` is neither keyword alone: a restricted scope.
    assert_eq!(load.visibility, Some(Visibility::Restricted));
    assert_eq!(load.modifiers, [Modifier::Async, Modifier::Override]);
    assert_eq!(load.generics.as_deref(), Some("<K>"));
    assert_eq!(
        load.return_type.as_ref().map(|t| t.text.as_str()),
        Some("Task<List<K>>")
    );
    assert_eq!(
        params(&load),
        ["id:int=3:positional", "names:string[]=-:variadic"]
    );
}

#[test]
fn an_extension_method_has_its_this_parameter_as_receiver() {
    let parsed = parsed();
    let shout = detail(&parsed, "Shout", 7);
    assert_eq!(
        shout.signature.as_deref(),
        Some("public static string Shout(this string s, ref int n, out int m)")
    );
    assert_eq!(shout.modifiers, [Modifier::Static]);
    assert_eq!(
        params(&shout),
        [
            "s:string=-:receiver",
            "n:int=-:positional",
            "m:int=-:positional"
        ]
    );
}

#[test]
fn without_an_access_keyword_visibility_is_the_language_default() {
    let parsed = parsed();
    // An explicit interface implementation is reached only through it.
    assert_eq!(detail(&parsed, "Run", 8).visibility, None);
    assert_eq!(
        detail(&parsed, "Run", 14).visibility,
        Some(Visibility::Public)
    );
    assert_eq!(
        detail(&parsed, "Inner", 12).visibility,
        Some(Visibility::Private)
    );
    assert_eq!(
        detail(&parsed, "IApi", 14).visibility,
        Some(Visibility::Internal)
    );
    let op = detail(&parsed, "Op", 17);
    assert_eq!(op.signature.as_deref(), Some("delegate int Op(int a)"));
    assert_eq!(op.visibility, Some(Visibility::Internal));
    assert_eq!(
        op.return_type.as_ref().map(|t| t.text.as_str()),
        Some("int")
    );
}

#[test]
fn types_and_members_get_their_header_without_attributes_or_body() {
    let parsed = parsed();
    let repo = detail(&parsed, "Repo", 2);
    assert_eq!(
        repo.signature.as_deref(),
        Some("public abstract partial class Repo<T> : Base, IApi where T : class")
    );
    assert_eq!(repo.modifiers, [Modifier::Abstract]);
    assert_eq!(repo.generics.as_deref(), Some("<T>"));
    let constructor = detail(&parsed, "Repo", 9);
    assert_eq!(constructor.signature.as_deref(), Some("public Repo(int x)"));
    assert!(constructor.return_type.is_none());
    for (name, signature) in [
        ("Limit", "private static readonly int Limit"),
        ("Max", "private static readonly int Max"),
    ] {
        let field = detail(&parsed, name, 4);
        assert_eq!(field.signature.as_deref(), Some(signature));
        assert_eq!(field.visibility, Some(Visibility::Private));
        assert_eq!(field.modifiers, [Modifier::Readonly, Modifier::Static]);
    }
    assert_eq!(
        detail(&parsed, "Name", 5).signature.as_deref(),
        Some("public string Name")
    );
    let indexer = detail(&parsed, "this", 10);
    assert_eq!(indexer.signature.as_deref(), Some("public int this[int i]"));
    assert_eq!(params(&indexer), ["i:int=-:positional"]);
    assert_eq!(
        detail(&parsed, "Changed", 11).signature.as_deref(),
        Some("public event Action Changed")
    );
    assert_eq!(
        detail(&parsed, "Point", 15).signature.as_deref(),
        Some("public record Point(int X, int Y)")
    );
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-csharp-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Repo.cs"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CSharpParser));
    let load = index
        .symbol_details("src/Repo.cs", 6, "LoadAsync")
        .unwrap()
        .unwrap();
    assert_eq!(load.visibility.as_deref(), Some("restricted"));
    assert_eq!(load.parameters.len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}
