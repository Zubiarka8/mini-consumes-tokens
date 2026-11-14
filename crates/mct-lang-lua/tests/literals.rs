//! String literals worth an exact-text search: each quoted or long
//! (`[[…]]`) string the shared prose filter keeps, on the line its text
//! starts on. Escapes are resolved in a quoted string; a long string keeps
//! its backslashes.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::QueryScope;
use mct_lang_lua::LuaParser;

const SOURCE: &str = r#"local api = require("app.api")

local function connect(url)
  log("first line\nsecond \"quoted\" line")
  error('Error de conexión con la BD: ' .. url)
end

local usage = [==[
Usage: tool C:\path\to\file
spans two lines]==]

return { connect = connect }
"#;

#[test]
fn prose_strings_are_literals() {
    let parsed = mct_corpus::parse_source(&LuaParser, "app/db.lua", "Lua", SOURCE);
    let literals: Vec<(&str, u32)> = parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect();
    assert_eq!(
        literals,
        [
            ("first line second \"quoted\" line", 4),
            ("Error de conexión con la BD:", 5),
            ("Usage: tool C:\\path\\to\\file spans two lines", 9),
        ]
    );
}

#[test]
fn an_exact_phrase_finds_the_literal_and_its_function() {
    let root = std::env::temp_dir().join(format!("mct-lua-literals-{}", std::process::id()));
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("app/db.lua"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(LuaParser));
    let hits: Vec<String> = index
        .search_literals("conexión con la BD", QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.symbol))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["app/db.lua:5 Some((\"connect\", \"function\"))"]);
}
