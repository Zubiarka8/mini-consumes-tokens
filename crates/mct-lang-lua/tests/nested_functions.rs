//! A named function declared inside another function belongs to it: its
//! `parent` is the innermost enclosing named function, as the Python and
//! JS/TS nested functions are. A dotted or colon name keeps its table as
//! the parent, and a function at file level has none.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_lua::LuaParser;

const SOURCE: &str = r#"
local function outer()
  local function helper()
    local function deepest() return 1 end
    return deepest()
  end
  function global_in_outer() return 2 end
  local cb = function()
    local function in_callback() return 3 end
    return in_callback()
  end
  return helper() + cb()
end

local M = {}
function M.run()
  local function inside_run() return 4 end
  function M.late() return inside_run() end
end
function M:method()
  local function inside_method() return 5 end
  return inside_method()
end
local function top() return outer() end
"#;

fn parents() -> Vec<(String, Option<String>)> {
    mct_corpus::parse_source(&LuaParser, "src/m.lua", "Lua", SOURCE)
        .symbols
        .into_iter()
        .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        .map(|s| (s.name, s.parent))
        .collect()
}

#[test]
fn a_named_function_inside_a_function_has_it_as_parent() {
    let found = parents();
    let parent_of = |name: &str| {
        found
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("{name} missing from {found:?}"))
            .1
            .as_deref()
    };
    assert_eq!(parent_of("helper"), Some("outer"));
    assert_eq!(parent_of("deepest"), Some("helper"));
    assert_eq!(parent_of("global_in_outer"), Some("outer"));
    // An anonymous function is transparent: the named one around it counts.
    assert_eq!(parent_of("in_callback"), Some("outer"));
    assert_eq!(parent_of("inside_run"), Some("run"));
    assert_eq!(parent_of("inside_method"), Some("method"));
}

#[test]
fn tables_and_file_level_functions_keep_their_parent() {
    let found = parents();
    let parent_of = |name: &str| found.iter().find(|(n, _)| n == name).unwrap().1.as_deref();
    assert_eq!(parent_of("outer"), None);
    assert_eq!(parent_of("top"), None);
    assert_eq!(parent_of("run"), Some("M"));
    assert_eq!(parent_of("method"), Some("M"));
    // `function M.late()` inside `M.run` is still a member of `M`.
    assert_eq!(parent_of("late"), Some("M"));
}

#[test]
fn calls_to_a_nested_function_still_name_it() {
    let parsed = mct_corpus::parse_source(&LuaParser, "src/m.lua", "Lua", SOURCE);
    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    for callee in [
        "helper",
        "deepest",
        "in_callback",
        "inside_run",
        "inside_method",
    ] {
        assert!(calls.contains(&callee), "{callee} missing from {calls:?}");
    }
}
