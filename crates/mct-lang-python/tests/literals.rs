//! String literals worth an exact-text search: each fixed fragment of a
//! string (between f-string holes) that the shared prose filter keeps, on
//! the line it starts on, with escapes resolved unless the string is raw. A
//! docstring, or any string standing alone as a statement, is
//! documentation, not a value, and is skipped.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::QueryScope;
use mct_lang_python::PythonParser;

const SOURCE: &str = r#""""Module docstring that reads like prose."""
KEY = "relative_path"
TOKEN = "Xk9mQ2vR7tLp4wZs8bNc3hJd"


def connect(url):
    """Open a connection to the database server."""
    log("first line\nsecond \"quoted\" line")
    raise ValueError(f"Error de conexión con la BD: {url} (retry {n} failed)")


def braces():
    return f"Literal {{braces}} stay in the text"


def raw():
    return r"Raw text keeps \n as written here"


def multi():
    return """
        Multi-line message
        spans two lines"""
"#;

#[test]
fn prose_fragments_are_literals_and_docstrings_are_not() {
    let parsed = mct_corpus::parse_source(&PythonParser, "db.py", "Python", SOURCE);
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
            ("Literal {braces} stay in the text", 13),
            ("Raw text keeps \\n as written here", 17),
            ("Multi-line message spans two lines", 22),
        ]
    );
}

#[test]
fn an_exact_phrase_finds_the_literal_and_its_function() {
    let root = std::env::temp_dir().join(format!("mct-py-literals-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("db.py"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PythonParser));
    let hit = |phrase: &str| -> Vec<String> {
        index
            .search_literals(phrase, QueryScope::default())
            .unwrap()
            .into_iter()
            .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.symbol))
            .collect()
    };
    let found = hit("conexión con la BD");
    let docstring = hit("connection to the database");
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(found, ["db.py:9 Some((\"connect\", \"function\"))"]);
    assert!(docstring.is_empty(), "{docstring:?}");
}
