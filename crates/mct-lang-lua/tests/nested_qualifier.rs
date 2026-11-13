//! A class of this file used as a call's qualifier (`Animal.new()`,
//! `W.Widget.make()`, `Animal:speak()`) is referenced by the caller. The
//! evidence is the file's own declaration: a qualifier shadowed by a local or
//! a parameter, a table that is no class, and a class of another file named
//! the same in a file that never declares it reference nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_lang_lua::LuaParser;

const ZOO: &str = r#"local W = {}
W.Widget = {}
W.Widget.__index = W.Widget
function W.Widget.make() return 1 end
local Animal = {}
Animal.__index = Animal
function Animal.new() return 1 end
function Animal:speak() return 2 end
local util = {}
function util.f() return 3 end
local a = Animal.new()
local w = W.Widget.make()
a = Animal:speak()
util.f()
local function shadowed(Animal)
  return Animal.new()
end
do
  local W = {}
  W.Widget.make()
end
return W
"#;

#[test]
fn a_class_of_this_file_used_as_a_qualifier_is_referenced() {
    let root = std::env::temp_dir().join(format!("mct-lua-qualifier-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("zoo.lua"), ZOO).unwrap();
    std::fs::write(
        root.join("use.lua"),
        "local x = Animal.new()\nlocal y = W.Widget.make()\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(LuaParser));
    let mut hits: Vec<String> = ["Animal", "Widget", "W", "util"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "references")
        .map(|h| {
            format!(
                "{}:{} {} {:?}",
                h.relative_path, h.line, h.to_name, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "zoo.lua:11 Animal Resolved",
            "zoo.lua:12 Widget Resolved",
            "zoo.lua:13 Animal Resolved",
        ]
    );
}
