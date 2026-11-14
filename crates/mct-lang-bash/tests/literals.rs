//! String literals worth an exact-text search: each run of fixed text of a
//! double-quoted string, between `$x` / `${…}` / `$(…)` expansions, and each
//! single-quoted or ANSI-C (`$'…'`) string, that the shared prose filter
//! keeps, on the line it starts on. In `"…"` only `\"`, `\\`, `` \` ``,
//! `\$` and a backslash-newline are escapes; `'…'` has none. Unquoted words
//! are command arguments, not strings.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::QueryScope;
use mct_lang_bash::BashParser;

const SOURCE: &str = r#"#!/usr/bin/env bash
source "./lib/common.sh"

connect() {
  echo "first line \"quoted\" and C:\data kept"
  echo "Error de conexión con la BD: $url (retry ${n} at $(date) failed)" >&2
  echo 'it is stored under C:\path here' $'tab\tseparated text'
  echo plain words are no string
}
"#;

#[test]
fn prose_strings_are_literals() {
    let parsed = mct_corpus::parse_source(&BashParser, "bin/db.sh", "Bash", SOURCE);
    let literals: Vec<(&str, u32)> = parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect();
    assert_eq!(
        literals,
        [
            ("first line \"quoted\" and C:\\data kept", 5),
            ("Error de conexión con la BD:", 6),
            ("it is stored under C:\\path here", 7),
            ("tab separated text", 7),
        ]
    );
}

#[test]
fn an_exact_phrase_finds_the_literal_and_its_function() {
    let root = std::env::temp_dir().join(format!("mct-bash-literals-{}", std::process::id()));
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::write(root.join("bin/db.sh"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(BashParser));
    let hits: Vec<String> = index
        .search_literals("conexión con la BD", QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.symbol))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["bin/db.sh:6 Some((\"connect\", \"function\"))"]);
}
