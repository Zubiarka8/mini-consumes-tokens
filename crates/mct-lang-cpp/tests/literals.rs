//! String literals worth an exact-text search: each string or raw
//! (`R"(…)"`) string the shared prose filter keeps, on the line its text
//! starts on. Escapes are resolved unless the string is raw. An `#include`
//! path is no message and never passes the filter.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::QueryScope;
use mct_lang_cpp::CppParser;

const SOURCE: &str = r#"#include "db/client.h"

namespace app {

void connect(const std::string& url) {
    log("first line\nsecond \"quoted\" line");
    throw std::runtime_error("Error de conexión con la BD: " + url);
}

const char* usage = R"(Usage: tool C:\path\to\file
spans two lines)";

}
"#;

#[test]
fn prose_strings_are_literals() {
    let parsed = mct_corpus::parse_source(&CppParser, "src/db.cpp", "C++", SOURCE);
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
            ("Usage: tool C:\\path\\to\\file spans two lines", 10),
        ]
    );
}

#[test]
fn an_exact_phrase_finds_the_literal_and_its_function() {
    let root = std::env::temp_dir().join(format!("mct-cpp-literals-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/db.cpp"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CppParser));
    let hits: Vec<String> = index
        .search_literals("conexión con la BD", QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.symbol))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["src/db.cpp:7 Some((\"connect\", \"function\"))"]);
}
