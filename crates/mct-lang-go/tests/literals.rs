//! String literals worth an exact-text search: each interpreted (`"…"`) or
//! raw (`` `…` ``) string the shared prose filter keeps, on the line it
//! starts on. Escapes in an interpreted string are resolved; a raw string
//! keeps its backslashes. A struct field tag (`json:"id" db:"user_id"`) is
//! metadata for reflection, not a message, and is skipped.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::QueryScope;
use mct_lang_go::GoParser;

const SOURCE: &str = r#"package db

import "github.com/acme/store/api"

type User struct {
	ID string `json:"id" db:"user_id"`
}

func Connect(url string) error {
	log.Printf("first line\nsecond \"quoted\" line")
	return fmt.Errorf("Error de conexión con la BD: %s", url)
}

const usage = `
  Usage: tool [flags] C:\path\to\file
  spans two lines`
"#;

#[test]
fn prose_strings_are_literals_and_field_tags_are_not() {
    let parsed = mct_corpus::parse_source(&GoParser, "db/db.go", "Go", SOURCE);
    let literals: Vec<(&str, u32)> = parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect();
    assert_eq!(
        literals,
        [
            ("first line second \"quoted\" line", 10),
            ("Error de conexión con la BD: %s", 11),
            ("Usage: tool [flags] C:\\path\\to\\file spans two lines", 15),
        ]
    );
}

#[test]
fn an_exact_phrase_finds_the_literal_and_its_function() {
    let root = std::env::temp_dir().join(format!("mct-go-literals-{}", std::process::id()));
    std::fs::create_dir_all(root.join("db")).unwrap();
    std::fs::write(root.join("db/db.go"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(GoParser));
    let hits: Vec<String> = index
        .search_literals("conexión con la BD", QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.symbol))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["db/db.go:11 Some((\"Connect\", \"function\"))"]);
}
