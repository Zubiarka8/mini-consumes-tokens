//! D9: a call to a Lua standard global (`setmetatable`, `pairs`,
//! `math.floor`) that nothing in the file binds can only be the host's, so
//! it is external, never a same-named definition in another file. A
//! `local setmetatable = setmetatable` elsewhere is that file's own local.
//! A name the file binds anywhere (a local, a parameter, a global
//! assignment, `function type()`) is not proven, and neither is anything in
//! a file that touches the environment (`_ENV`, `_G`, `setfenv`).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_lua::LuaParser;

const MONEY: &str = r#"
local floor = math.floor
local setmetatable = setmetatable
local Money = {}
function Money.new(minor)
  return setmetatable({ minor = floor(minor) }, Money)
end
return Money
"#;

const ORDER: &str = r#"
local Order = {}
function Order.new(lines)
  local total = 0
  for _, line in ipairs(lines) do
    total = total + math.floor(line.amount)
  end
  print(tostring(total))
  return setmetatable({ lines = lines, total = total }, Order)
end
function Order.check(type)
  return type(1)
end
function tostring(x) return "" end
return Order
"#;

const SANDBOX: &str = r#"
local _ENV = { print = function() end }
print("hi")
"#;

fn parsed(path: &str, source: &str) -> ParsedFile {
    mct_corpus::parse_source(&LuaParser, path, "Lua", source)
}

/// Every call as `name` plus ` external` when the parser proved it so.
fn calls(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .map(|(i, r)| {
            let external = parsed
                .relation_targets
                .iter()
                .any(|t| t.relation == i && t.external);
            format!("{}{}", r.to_name, if external { " external" } else { "" })
        })
        .collect()
}

#[test]
fn an_unbound_standard_global_is_external() {
    assert_eq!(
        calls(&parsed("app/order.lua", ORDER)),
        [
            "ipairs external",
            "floor external",
            "print external",
            "tostring",
            "setmetatable external",
            "type",
        ]
    );
}

#[test]
fn a_name_the_file_binds_is_not_proven() {
    assert_eq!(
        calls(&parsed("app/money.lua", MONEY)),
        ["setmetatable", "floor"]
    );
}

#[test]
fn a_file_that_touches_the_environment_marks_nothing() {
    assert_eq!(calls(&parsed("app/sandbox.lua", SANDBOX)), ["print"]);
}

#[test]
fn another_file_s_local_alias_is_never_the_target() {
    let root = std::env::temp_dir().join(format!("mct-lua-std-{}", std::process::id()));
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("app/money.lua"), MONEY).unwrap();
    std::fs::write(root.join("app/order.lua"), ORDER).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(LuaParser));
    let mut hits: Vec<String> = ["setmetatable", "floor"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| {
            format!(
                "{} {}:{} {:?}",
                h.to_name, h.relative_path, h.line, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "floor app/money.lua:6 Resolved",
            "floor app/order.lua:6 External",
            "setmetatable app/money.lua:6 Resolved",
            "setmetatable app/order.lua:9 External",
        ]
    );
}
