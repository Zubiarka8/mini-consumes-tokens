//! String literals worth an exact-text search: each run of fixed text of a
//! single-quoted, double-quoted, heredoc or nowdoc string, between `$x` /
//! `{$…}` interpolations, that the shared prose filter keeps, on the line
//! it starts on. A single-quoted string keeps backslashes other than `\'`
//! and `\\`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::QueryScope;
use mct_lang_php::PhpParser;

const SOURCE: &str = r#"<?php
namespace App;

function connect($url) {
    log('it\'s stored under C:\data now');
    throw new \RuntimeException("Error de conexión con la BD: $url (retry {$n->x} failed)\n");
}

$usage = <<<EOT
  Usage: tool $name
  spans two lines
  EOT;
$raw = <<<'EOT'
  Nowdoc text keeps $name as written
  EOT;
"#;

#[test]
fn prose_fragments_are_literals() {
    let parsed = mct_corpus::parse_source(&PhpParser, "src/db.php", "PHP", SOURCE);
    let literals: Vec<(&str, u32)> = parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect();
    assert_eq!(
        literals,
        [
            ("it's stored under C:\\data now", 5),
            ("Error de conexión con la BD:", 6),
            ("Usage: tool", 10),
            ("spans two lines", 11),
            ("Nowdoc text keeps $name as written", 14),
        ]
    );
}

#[test]
fn an_exact_phrase_finds_the_literal_and_its_function() {
    let root = std::env::temp_dir().join(format!("mct-php-literals-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/db.php"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PhpParser));
    let hits: Vec<String> = index
        .search_literals("conexión con la BD", QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.symbol))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["src/db.php:6 Some((\"connect\", \"function\"))"]);
}
