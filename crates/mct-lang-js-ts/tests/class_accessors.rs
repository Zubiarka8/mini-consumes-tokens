//! A class accessor pair (`get total()` / `set total(v)`) is one `field` of
//! its class, as a TS property is: the first accessor declares it, every
//! accessor's body is owned by it, and the field spans them all. An
//! object literal's accessor stays a `function`, as its methods do. A
//! typed getter (`get repo(): Repo`) types the field, so `this.repo.find()`
//! reaches `Repo.find`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"import { Repo } from "./repo";
export class Cart {
  get total(): number {
    return sum();
  }
  set total(v: number) {
    log(v);
  }
  get repo(): Repo {
    return make();
  }
  static get empty() {
    return 0;
  }
  load() {
    return this.repo.find();
  }
}
export const shape = {
  get area() { return 1; },
};
"#;

/// Name, kind, parent, line and end line of one symbol.
type Row<'a> = (&'a str, SymbolKind, Option<&'a str>, u32, Option<u32>);

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JsTsParser, "src/cart.ts", "TypeScript", SOURCE)
}

#[test]
fn an_accessor_pair_is_one_field_owning_both_bodies() {
    let parsed = parsed();
    let symbols: Vec<Row> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind != SymbolKind::Module)
        .map(|s| {
            (
                s.name.as_str(),
                s.kind,
                s.parent.as_deref(),
                s.location.line,
                s.location.end_line,
            )
        })
        .collect();
    assert_eq!(
        symbols,
        [
            ("Cart", SymbolKind::Class, None, 2, Some(18)),
            ("total", SymbolKind::Field, Some("Cart"), 3, Some(8)),
            ("repo", SymbolKind::Field, Some("Cart"), 9, Some(11)),
            ("empty", SymbolKind::Field, Some("Cart"), 12, Some(14)),
            ("load", SymbolKind::Method, Some("Cart"), 15, Some(17)),
            ("shape", SymbolKind::Constant, None, 19, Some(21)),
            ("area", SymbolKind::Function, None, 20, Some(20)),
        ]
    );
    let total = parsed
        .symbols
        .iter()
        .find(|s| s.name == "total")
        .unwrap()
        .id;
    let mut calls: Vec<&str> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls && r.from == total)
        .map(|r| r.to_name.as_str())
        .collect();
    calls.sort_unstable();
    assert_eq!(calls, ["log", "sum"]);
}

#[test]
fn a_typed_getter_types_the_field_for_this_calls() {
    let root = std::env::temp_dir().join(format!("mct-ts-accessors-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/cart.ts"), SOURCE).unwrap();
    std::fs::write(
        root.join("src/repo.ts"),
        "export class Repo {\n  find() { return 1; }\n}\nexport class Store {\n  find() { return 2; }\n}\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    let hits: Vec<String> = index
        .find_references("find")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["src/cart.ts:16 Resolved"]);
}
