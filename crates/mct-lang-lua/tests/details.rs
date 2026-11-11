//! Declaration details (ADR-034, issue #321) the Lua parser proves: the
//! header up to the parameter list, parameters (`function T:m()` has the
//! implicit `self` receiver, `...` is variadic) and, since Lua writes no
//! types, the LuaLS annotations right above (`---@param name type`,
//! `---@return type`) as `doc` types. Lua has no visibility syntax.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, Provenance, SymbolDetail, SymbolKind};
use mct_lang_lua::LuaParser;

const SOURCE: &str = r#"
local M = {}
--- Loads things.
---@param id integer the id
---@param ... string
---@return table|nil
function M:load(id, ...) return nil end
local function helper(a) end
M.run = function(x) end
local T = {
  ---@param n number
  size = function(n) end,
}
---@param s string?
---@return boolean ok
---@return string? err
local check = function(s) end
return M
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&LuaParser, "src/m.lua", "Lua", SOURCE)
}

fn detail(parsed: &ParsedFile, name: &str, line: u32) -> SymbolDetail {
    let symbol = parsed
        .symbols
        .iter()
        .find(|s| s.name == name && s.location.line == line && s.kind != SymbolKind::Module)
        .unwrap_or_else(|| panic!("no {name} on line {line}"));
    parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == symbol.id)
        .cloned()
        .unwrap_or_else(|| panic!("no detail for {name}"))
        .normalized()
}

/// `name:type:kind` per parameter.
fn params(detail: &SymbolDetail) -> Vec<String> {
    detail
        .parameters
        .iter()
        .map(|p| {
            format!(
                "{}:{}:{}",
                p.name.as_deref().unwrap_or("_"),
                p.type_text.as_ref().map_or("?", |t| t.text.as_str()),
                p.kind.as_str()
            )
        })
        .collect()
}

#[test]
fn a_method_has_the_implicit_self_and_annotated_types() {
    let parsed = parsed();
    let load = detail(&parsed, "load", 7);
    assert_eq!(load.signature.as_deref(), Some("function M:load(id, ...)"));
    assert_eq!(load.visibility, None);
    assert_eq!(
        params(&load),
        [
            "self:?:receiver",
            "id:integer:positional",
            "_:string:variadic"
        ]
    );
    assert!(load
        .parameters
        .iter()
        .filter_map(|p| p.type_text.as_ref())
        .all(|t| t.provenance == Provenance::Doc));
    let returns = load.return_type.unwrap();
    assert_eq!(
        (returns.text.as_str(), returns.provenance),
        ("table|nil", Provenance::Doc)
    );
}

#[test]
fn unannotated_functions_have_parameters_without_types() {
    let parsed = parsed();
    let helper = detail(&parsed, "helper", 8);
    assert_eq!(
        helper.signature.as_deref(),
        Some("local function helper(a)")
    );
    assert_eq!(params(&helper), ["a:?:positional"]);
    assert!(helper.return_type.is_none());
    assert_eq!(
        detail(&parsed, "run", 9).signature.as_deref(),
        Some("M.run = function(x)")
    );
}

#[test]
fn table_fields_and_local_function_values_read_their_annotations() {
    let parsed = parsed();
    let size = detail(&parsed, "size", 12);
    assert_eq!(size.signature.as_deref(), Some("size = function(n)"));
    assert_eq!(params(&size), ["n:number:positional"]);
    let check = detail(&parsed, "check", 17);
    assert_eq!(
        check.signature.as_deref(),
        Some("local check = function(s)")
    );
    assert_eq!(params(&check), ["s:string?:positional"]);
    // Every `@return` is one value of a multiple return.
    assert_eq!(
        check.return_type.map(|t| t.text).as_deref(),
        Some("boolean, string?")
    );
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-lua-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/m.lua"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(LuaParser));
    let load = index
        .symbol_details("src/m.lua", 7, "load")
        .unwrap()
        .unwrap();
    assert_eq!(load.return_source.as_deref(), Some("doc"));
    assert_eq!(load.parameters.len(), 3);
    std::fs::remove_dir_all(root).unwrap();
}
