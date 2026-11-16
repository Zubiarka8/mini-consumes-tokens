//! A function used by name as a value (`table.sort(t, cmp)`,
//! `{ on = handler }`) is a `references` relation from where it is used, so
//! a callback is not dead code. Only a name that is a function of this file
//! counts: a parameter, a plain variable or an unknown name records nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_lua::LuaParser;

const SOURCE: &str = r#"local function cmp(a, b) return a < b end
local function handler() end
function global_cb() end
local count = 3

local function sorted(t, cmp)
  table.sort(t, cmp)
  return t
end

local t = { 3, 1 }
table.sort(t, cmp)
local registry = { on = handler, size = count, other = unknown }
run(global_cb, count, unknown)
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&LuaParser, "main.lua", "Lua", SOURCE)
}

fn references(file: &ParsedFile) -> Vec<(String, String, u32)> {
    file.relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| {
            let from = file.symbols.iter().find(|s| s.id == r.from).unwrap();
            (from.name.clone(), r.to_name.clone(), r.location.line)
        })
        .collect()
}

#[test]
fn a_function_passed_or_stored_by_name_is_referenced() {
    let refs = references(&parsed());
    for expected in [("cmp", 12), ("handler", 13), ("global_cb", 14)] {
        assert!(
            refs.iter()
                .any(|(_, to, line)| (to.as_str(), *line) == expected),
            "{expected:?} missing from {refs:?}"
        );
    }
}

#[test]
fn a_parameter_a_variable_or_an_unknown_name_is_not_referenced() {
    let refs = references(&parsed());
    // `cmp` on line 7 is the parameter, which shadows the function.
    assert!(!refs.iter().any(|(_, to, line)| to == "cmp" && *line == 7));
    for name in ["count", "unknown"] {
        assert!(
            !refs.iter().any(|(_, to, _)| to == name),
            "{name}: {refs:?}"
        );
    }
}
