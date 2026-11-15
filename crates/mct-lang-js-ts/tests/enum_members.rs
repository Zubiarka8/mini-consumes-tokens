//! A TypeScript enum member (`Red`, `Green = 2`, `'dark-blue' = "x"`) is a
//! `field` of its enum: the property `Color.Red` the enum object has at run
//! time, as a Java, Kotlin or PHP enum constant is a field. A computed or
//! numeric name declares nothing, and a call in a member's initializer is a
//! call from the enum. A type annotation never names a field or an enum
//! member, so an interface named like a member stays the only candidate.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"export enum Color {
  Red,
  Green = 2,
  'dark-blue' = "db",
  Size = "abc".length,
  Seed = seed(),
}
const enum Flag { A = 1 << 0, B = A | 2 }
declare enum Remote { X }
function seed() { return 4; }
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JsTsParser, "src/color.ts", "TypeScript", SOURCE)
}

#[test]
fn each_enum_member_is_a_field_of_its_enum() {
    let parsed = parsed();
    let mut fields: Vec<(String, Option<String>, u32)> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Field)
        .map(|s| (s.name.clone(), s.parent.clone(), s.location.line))
        .collect();
    fields.sort_by_key(|c| c.2);
    let member =
        |name: &str, parent: &str, line: u32| (name.to_string(), Some(parent.to_string()), line);
    assert_eq!(
        fields,
        [
            member("Red", "Color", 2),
            member("Green", "Color", 3),
            member("dark-blue", "Color", 4),
            member("Size", "Color", 5),
            member("Seed", "Color", 6),
            member("A", "Flag", 8),
            member("B", "Flag", 8),
            member("X", "Remote", 9),
        ]
    );
}

#[test]
fn a_call_in_a_member_initializer_is_a_call_from_the_enum() {
    let parsed = parsed();
    let color = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Color" && s.kind == SymbolKind::Enum)
        .unwrap()
        .id;
    let calls: Vec<(&str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls && r.from == color)
        .map(|r| (r.to_name.as_str(), r.location.line))
        .collect();
    assert_eq!(calls, [("seed", 6)]);
}

#[test]
fn a_type_annotation_never_reaches_an_enum_member_or_a_field() {
    let root = std::env::temp_dir().join(format!("mct-ts-enum-members-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/order.ts"),
        concat!(
            "export enum State { Paid = \"PAID\" }\n",
            "export interface Paid { amount: number }\n",
            "class Cart { amount = 0; total(): amount { return this.amount; } }\n",
            "export function pay(): Paid { return { amount: 1 }; }\n",
        ),
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    let mut hits: Vec<String> = ["Paid", "amount"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "references")
        .map(|h| {
            format!(
                "{}:{} {} {:?}",
                h.relative_path, h.line, h.to_name, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            // `amount` names no type here, and no field can be one.
            "src/order.ts:3 amount Unresolved",
            "src/order.ts:4 Paid Resolved",
        ]
    );
}
