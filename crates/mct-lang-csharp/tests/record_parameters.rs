//! A record's positional parameter (`record Point(int X, int Y)`) is a
//! public property the compiler generates, so it is a `field` of the
//! record, `record struct` included, with `int Y` as its signature. A
//! property the body declares with that name replaces the generated one, so
//! it stays the only row. A class or struct primary-constructor parameter
//! (`class Plain(int seed)`) is a captured value, never a member.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, SymbolKind, Visibility};
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"namespace Shop;

public record Point(int X, [property: Json("y")] int Y = 0)
{
    public int X { get; init; } = X;
}

public readonly record struct Size(int W);

public class Plain(int seed) { }
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CSharpParser, "Shop/Point.cs", "C#", SOURCE)
}

#[test]
fn positional_record_parameters_are_fields() {
    let parsed = parsed();
    let fields: Vec<(&str, Option<&str>, u32, Option<String>)> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Field)
        .map(|s| {
            let signature = parsed
                .symbol_details
                .iter()
                .find(|d| d.symbol == s.id)
                .and_then(|d| d.signature.clone());
            (
                s.name.as_str(),
                s.parent.as_deref(),
                s.location.line,
                signature,
            )
        })
        .collect();
    let field = |name, parent, line, signature: &str| {
        (name, Some(parent), line, Some(signature.to_string()))
    };
    assert_eq!(
        fields,
        [
            field("Y", "Point", 3, "int Y"),
            field("X", "Point", 5, "public int X"),
            field("W", "Size", 8, "int W"),
        ]
    );
}

#[test]
fn a_positional_field_is_public() {
    let parsed = parsed();
    let y = parsed.symbols.iter().find(|s| s.name == "Y").unwrap().id;
    let detail = parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == y)
        .unwrap();
    assert_eq!(detail.visibility, Some(Visibility::Public));
}

#[test]
fn a_constructor_call_never_reaches_a_positional_field() {
    // `record Box(Item Item)` gives `Box` a field `Item`; `new Item()`
    // builds the class, which a field can't be.
    let root = std::env::temp_dir().join(format!("mct-cs-record-{}", std::process::id()));
    std::fs::create_dir_all(root.join("Shop")).unwrap();
    std::fs::write(
        root.join("Shop/Box.cs"),
        "namespace Shop;\npublic class Item { }\npublic record Box(Item Item);\npublic static class Make { public static Item One() => new Item(); }\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, std::sync::Arc::new(CSharpParser));
    let hits: Vec<String> = index
        .find_references("Item")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["Shop/Box.cs:4 Resolved"]);
}
