//! `W.Widget.__index = W.Widget` makes the nested table `Widget` of `W` a
//! `class`, as `Widget.__index = Widget` does for a file-level table. Only
//! the self-referencing idiom counts: `W.Widget.__index = W.Other` or a
//! table of another file declares no class.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::SymbolKind;
use mct_lang_lua::LuaParser;

const SOURCE: &str = r#"local W = {}
W.Widget = {}
W.Widget.__index = W.Widget
W.Panel = {}
W.Panel.__index = W.Other
W.a = { b = {} }
W.a.b.__index = W.a.b
function W.Widget.new() return setmetatable({}, W.Widget) end
function W.Widget:render() end
Remote.Thing.__index = Remote.Thing
return W
"#;

#[test]
fn a_nested_table_wired_to_itself_is_a_class() {
    let parsed = mct_corpus::parse_source(&LuaParser, "ui.lua", "Lua", SOURCE);
    let tables: Vec<(&str, SymbolKind, Option<&str>)> = parsed
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Module | SymbolKind::Class))
        .map(|s| (s.name.as_str(), s.kind, s.parent.as_deref()))
        .collect();
    assert_eq!(
        tables,
        [
            ("ui", SymbolKind::Module, None),
            ("W", SymbolKind::Module, None),
            ("Widget", SymbolKind::Class, Some("W")),
            ("Panel", SymbolKind::Module, Some("W")),
            ("a", SymbolKind::Module, Some("W")),
        ]
    );
}
