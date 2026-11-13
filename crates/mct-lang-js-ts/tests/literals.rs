//! String literals worth an exact-text search: each run of fixed text of a
//! string or template literal (between `${…}` holes) that the shared prose
//! filter keeps, on the line it starts on, with escapes resolved. A
//! directive (`"use strict"`) or other string standing alone as a statement
//! is skipped, and so is a JSX `className` / `class` list, which names CSS
//! classes rather than saying anything.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::QueryScope;
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#""use strict";
import { api } from "./api-client";
const KEY = "relative_path";
export function connect(url) {
  log("first line\nsecond \"quoted\" line");
  throw new Error(`Error de conexión con la BD: ${url} (retry ${n} failed)`);
}
export function Panel() {
  return <input className="flex items-center gap-2" placeholder="Search the product catalog" />;
}
const multi = `
  Multi-line message
  spans two lines`;
"#;

#[test]
fn prose_fragments_are_literals_and_directives_are_not() {
    let parsed = mct_corpus::parse_source(&JsTsParser, "src/db.jsx", "JavaScript", SOURCE);
    let literals: Vec<(&str, u32)> = parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect();
    assert_eq!(
        literals,
        [
            ("first line second \"quoted\" line", 5),
            ("Error de conexión con la BD:", 6),
            ("Search the product catalog", 9),
            ("Multi-line message spans two lines", 12),
        ]
    );
}

#[test]
fn an_exact_phrase_finds_the_literal_and_its_function() {
    let root = std::env::temp_dir().join(format!("mct-js-literals-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/db.jsx"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    let hits: Vec<String> = index
        .search_literals("conexión con la BD", QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.symbol))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["src/db.jsx:6 Some((\"connect\", \"function\"))"]);
}
