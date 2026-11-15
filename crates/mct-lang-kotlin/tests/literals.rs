//! String literals worth an exact-text search: each run of fixed text of a
//! string or raw (`"""…"""`) string, between `$x` / `${…}` templates, that
//! the shared prose filter keeps, on the line it starts on. Escapes are
//! resolved in a plain string; a raw string keeps its backslashes. A `$`
//! that starts no name (`$ 5`) is text.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::QueryScope;
use mct_lang_kotlin::KotlinParser;

const SOURCE: &str = r#"package app

class Db {
    @Suppress("UNCHECKED_CAST")
    fun connect(url: String) {
        log("first line\nsecond \"quoted\" line")
        error("Error de conexión con la BD: $url (retry ${n + 1} failed)")
    }

    val usage = """
        Usage: tool C:\path\to\file
        spans two lines
    """
    val price = "price is $ 5 per unit"
}
"#;

#[test]
fn prose_fragments_are_literals() {
    let parsed = mct_corpus::parse_source(&KotlinParser, "app/Db.kt", "Kotlin", SOURCE);
    let literals: Vec<(&str, u32)> = parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect();
    assert_eq!(
        literals,
        [
            ("first line second \"quoted\" line", 6),
            ("Error de conexión con la BD:", 7),
            ("Usage: tool C:\\path\\to\\file spans two lines", 11),
            ("price is $ 5 per unit", 14),
        ]
    );
}

#[test]
fn an_exact_phrase_finds_the_literal_and_its_method() {
    let root = std::env::temp_dir().join(format!("mct-kotlin-literals-{}", std::process::id()));
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("app/Db.kt"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(KotlinParser));
    let hits: Vec<String> = index
        .search_literals("conexión con la BD", QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.symbol))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["app/Db.kt:7 Some((\"connect\", \"method\"))"]);
}
