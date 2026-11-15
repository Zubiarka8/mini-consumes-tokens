//! A TypeScript interface's method signature (`total(): number`) is a
//! `method` of the interface and its property signature (`readonly id:
//! string`) a `field`, each with its signature. Overloads give one row, the
//! first; a call, construct or index signature, a computed or numeric name
//! and a member of a nested object type (`opts: { x: number }`) declare
//! nothing. A call on a value of the interface's type (`cart: Cart`,
//! `cart.total()`) now reaches the signature.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, SymbolKind};
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"export interface Cart {
  readonly id: string;
  'item-count'?: number;
  opts: { x: number };
  total(): number;
  add(sku: string): void;
  add(sku: string, n: number): void;
  (): void;
  new (): Cart;
  [key: string]: unknown;
}
type Shape = { area(): number };
"#;

/// Name, kind, parent, line and signature of one member.
type Member = (String, SymbolKind, Option<String>, u32, Option<String>);

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JsTsParser, "src/cart.ts", "TypeScript", SOURCE)
}

#[test]
fn signatures_are_methods_and_fields_of_their_interface() {
    let parsed = parsed();
    let mut members: Vec<Member> = parsed
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Method | SymbolKind::Field))
        .map(|s| {
            let signature = parsed
                .symbol_details
                .iter()
                .find(|d| d.symbol == s.id)
                .and_then(|d| d.signature.clone());
            (
                s.name.clone(),
                s.kind,
                s.parent.clone(),
                s.location.line,
                signature,
            )
        })
        .collect();
    members.sort_by_key(|m| m.3);
    let member = |name: &str, kind: SymbolKind, line: u32, signature: &str| {
        (
            name.to_string(),
            kind,
            Some("Cart".to_string()),
            line,
            Some(signature.to_string()),
        )
    };
    assert_eq!(
        members,
        [
            member("id", SymbolKind::Field, 2, "readonly id: string"),
            member("item-count", SymbolKind::Field, 3, "'item-count'?: number"),
            member("opts", SymbolKind::Field, 4, "opts: { x: number }"),
            member("total", SymbolKind::Method, 5, "total(): number"),
            member("add", SymbolKind::Method, 6, "add(sku: string): void"),
        ]
    );
}

#[test]
fn a_call_on_a_value_of_the_interface_type_reaches_its_signature() {
    let root =
        std::env::temp_dir().join(format!("mct-ts-interface-members-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/cart.ts"), SOURCE).unwrap();
    std::fs::write(
        root.join("src/checkout.ts"),
        "import { Cart } from \"./cart\";\nexport function pay(cart: Cart) {\n  return cart.total();\n}\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    let hits: Vec<String> = index
        .find_references("total")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["src/checkout.ts:3 Resolved"]);
}

#[test]
fn a_call_with_no_proven_receiver_never_resolves_to_a_signature() {
    let root =
        std::env::temp_dir().join(format!("mct-ts-interface-unproven-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/panel.ts"),
        concat!(
            "export interface Props {\n",
            "  onSaved(n: number): void;\n",
            "  error: (message: string) => void;\n",
            "}\n",
            "export function panel({ onSaved }: Props) {\n",
            "  onSaved(1);\n",
            "}\n",
            "export const plugin = {\n",
            "  hook() { this.error(\"x\"); },\n",
            "};\n",
        ),
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    let mut hits: Vec<String> = ["onSaved", "error"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            // A destructured parameter holds a value, never a method.
            "src/panel.ts:6 Unresolved",
            // `this` in an object literal is whatever the caller binds: it
            // proves no field.
            "src/panel.ts:9 Unresolved",
        ]
    );
}
