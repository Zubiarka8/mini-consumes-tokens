//! A TS interface's property and method signatures, and an enum's members,
//! are symbols owned by their type. An interface's call, construct and
//! index signatures have no name, so they stay out; a class's overload
//! signatures (also `method_signature` in the grammar) stay dropped, and an
//! object type in a type alias is not a type declaration.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, SymbolKind, Visibility};
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"
interface Shape {
    readonly id: string;
    label?: string;
    'css-name': string;
    area(): number;
    get size(): number;
    (x: number): void;
    new (): Shape;
    [key: string]: unknown;
}
enum Color { Red, Green = "g", 'dark-blue' = 4, }
class Box {
    fit(x: number): void;
    fit(x: any) {}
}
type Point = { x: number; move(): void };
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JsTsParser, "src/shape.ts", "JavaScript/TypeScript", SOURCE)
}

/// `Parent.name kind` for every symbol with a parent.
fn members(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .symbols
        .iter()
        .filter_map(|s| {
            let parent = s.parent.as_deref()?;
            Some(format!("{parent}.{} {:?}", s.name, s.kind))
        })
        .collect()
}

#[test]
fn interface_and_enum_members_are_symbols_of_their_type() {
    assert_eq!(
        members(&parsed()),
        [
            "Shape.id Field",
            "Shape.label Field",
            "Shape.css-name Field",
            "Shape.area Method",
            "Shape.size Method",
            "Color.Red Field",
            "Color.Green Field",
            "Color.dark-blue Field",
            "Box.fit Method",
        ]
    );
}

#[test]
fn an_interface_member_is_public_with_its_signature() {
    let parsed = parsed();
    let detail = |name: &str| {
        let symbol = parsed
            .symbols
            .iter()
            .find(|s| s.name == name && s.parent.as_deref() == Some("Shape"))
            .unwrap();
        parsed
            .symbol_details
            .iter()
            .find(|d| d.symbol == symbol.id)
            .unwrap()
    };
    let area = detail("area");
    assert_eq!(area.signature.as_deref(), Some("area(): number"));
    assert_eq!(area.visibility, Some(Visibility::Public));
    let id = detail("id");
    assert_eq!(id.signature.as_deref(), Some("readonly id: string"));
    assert_eq!(id.visibility, Some(Visibility::Public));
}

#[test]
fn no_member_is_a_top_level_symbol() {
    let parsed = parsed();
    let top: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.parent.is_none() && s.kind != SymbolKind::Module)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(top, ["Shape", "Color", "Box", "Point"]);
}
