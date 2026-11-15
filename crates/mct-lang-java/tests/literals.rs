//! String literals worth an exact-text search: each string literal or text
//! block (`"""…"""`) the shared prose filter keeps, on the line its text
//! starts on, with escapes resolved.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::QueryScope;
use mct_lang_java::JavaParser;

const SOURCE: &str = r#"package app;

import java.util.List;

public class Db {
    @SuppressWarnings("unchecked")
    public void connect(String url) {
        log("first line\nsecond \"quoted\" line");
        throw new IllegalStateException("Error de conexión con la BD: " + url);
    }

    static final String USAGE = """
        Usage: tool [flags]
        spans two lines
        """;
}
"#;

#[test]
fn prose_strings_and_text_blocks_are_literals() {
    let parsed = mct_corpus::parse_source(&JavaParser, "app/Db.java", "Java", SOURCE);
    let literals: Vec<(&str, u32)> = parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect();
    assert_eq!(
        literals,
        [
            ("first line second \"quoted\" line", 8),
            ("Error de conexión con la BD:", 9),
            ("Usage: tool [flags] spans two lines", 13),
        ]
    );
}

#[test]
fn an_exact_phrase_finds_the_literal_and_its_method() {
    let root = std::env::temp_dir().join(format!("mct-java-literals-{}", std::process::id()));
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("app/Db.java"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JavaParser));
    let hits: Vec<String> = index
        .search_literals("conexión con la BD", QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.symbol))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["app/Db.java:9 Some((\"connect\", \"method\"))"]);
}
