//! `W.Dog = setmetatable({}, { __index = W.Animal })` at file level declares
//! the class `Dog` of `W` extending `Animal`, as the plain-name form
//! `local Dog = setmetatable({}, { __index = Animal })` declares `Dog`.
//! Without a base it stays an ordinary value, and inside a function it
//! declares nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_lua::LuaParser;

const SOURCE: &str = r#"local W = {}
W.Animal = {}
W.Animal.__index = W.Animal
function W.Animal.new() return 1 end
W.Dog = setmetatable({}, { __index = W.Animal })
W.Dog.__index = W.Dog
function W.Dog:bark() return 2 end
W.plain = setmetatable({}, {})
local function make()
  W.Cat = setmetatable({}, { __index = W.Animal })
end
return W
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&LuaParser, "zoo.lua", "Lua", SOURCE)
}

#[test]
fn a_nested_table_bound_by_setmetatable_is_a_class() {
    let parsed = parsed();
    let mut symbols: Vec<(&str, SymbolKind, Option<&str>, u32)> = parsed
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Class | SymbolKind::Module))
        .map(|s| {
            (
                s.name.as_str(),
                s.kind,
                s.parent.as_deref(),
                s.location.line,
            )
        })
        .collect();
    symbols.sort_by_key(|s| s.3);
    assert_eq!(
        symbols,
        [
            ("zoo", SymbolKind::Module, None, 1),
            ("W", SymbolKind::Module, None, 1),
            ("Animal", SymbolKind::Class, Some("W"), 2),
            ("Dog", SymbolKind::Class, Some("W"), 5),
        ]
    );
    let dog = parsed.symbols.iter().find(|s| s.name == "Dog").unwrap().id;
    let extends: Vec<&str> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Extends)
        .map(|r| {
            assert_eq!(r.from, dog);
            r.to_name.as_str()
        })
        .collect();
    assert_eq!(extends, ["Animal"]);
}

#[test]
fn the_extension_resolves_to_the_nested_base() {
    let root = std::env::temp_dir().join(format!("mct-lua-nested-mt-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("zoo.lua"), SOURCE).unwrap();
    std::fs::write(
        root.join("other.lua"),
        "local Animal = {}\nAnimal.__index = Animal\nreturn Animal\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(LuaParser));
    let hits: Vec<String> = index
        .find_references("Animal")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "extends")
        .map(|h| {
            let candidates: Vec<String> = h
                .candidates
                .iter()
                .map(|c| c.relative_path.clone())
                .collect();
            format!(
                "{}:{} {:?} {candidates:?}",
                h.relative_path, h.line, h.resolution
            )
        })
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["zoo.lua:5 Resolved [\"zoo.lua\"]"]);
}
