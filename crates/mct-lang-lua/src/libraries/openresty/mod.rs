//! OpenResty (`ngx_lua`) application-pattern coverage through the shared Lua
//! parser. Scope: what the generic parser records for the conventions of a
//! `resty` module, written after `tests/corpus/project/gateway/`. It does not
//! model the framework: `ngx.*` is an ordinary table whose calls are recorded
//! by name, `resty.*` modules are third-party imports that resolve to nothing
//! local, and the nginx.conf directives that bind a Lua file to a request
//! phase (`content_by_lua_file`, `access_by_lua_block`) are not Lua and are
//! not indexed, so a handler's entry point is not discoverable from here.

use mct_core::{LanguageParser, ParsedFile, RelationKind, SourceFile, SymbolKind};

use crate::LuaParser;

const MODULE: &str = r#"
local lock = require("resty.lock")
local cjson = require("cjson.safe")
local config = require("gateway.config")

local _M = { _VERSION = "0.1" }
local mt = { __index = _M }

function _M.new(dict_name)
  local dict = ngx.shared[dict_name]
  return setmetatable({ dict = dict }, mt)
end

function _M:take(key)
  local mutex = lock:new("locks")
  ngx.log(ngx.WARN, "take ", key)
  for _, rule in ipairs(config.rules) do
    ngx.say(cjson.encode(rule))
  end
  return ngx.exit(429)
end

return _M
"#;

fn parse() -> ParsedFile {
    LuaParser
        .parse(&SourceFile {
            relative_path: "gateway/limiter.lua".to_string(),
            contents: MODULE.to_string(),
        })
        .unwrap_or_default()
}

fn calls(parsed: &ParsedFile) -> Vec<&str> {
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect()
}

#[test]
fn a_resty_module_table_exposes_its_functions_as_methods() {
    let parsed = parse();
    let method = |name: &str| {
        parsed
            .symbols
            .iter()
            .find(|s| s.name == name && s.kind == SymbolKind::Method)
            .and_then(|s| s.parent.as_deref())
    };
    assert_eq!(method("new"), Some("_M"));
    assert_eq!(method("take"), Some("_M"));
}

#[test]
fn ngx_api_calls_are_recorded_by_name() {
    let parsed = parse();
    let recorded = calls(&parsed);
    for name in ["log", "say", "exit", "encode", "ipairs", "setmetatable"] {
        assert!(recorded.contains(&name), "{name}: {recorded:?}");
    }
}

#[test]
fn resty_requires_are_imports_carrying_their_package() {
    let parsed = parse();
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Imports)
        .map(|(i, r)| {
            let package = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.module.as_deref());
            (r.to_name.as_str(), package)
        })
        .collect();
    assert!(imports.contains(&("lock", Some("resty"))), "{imports:?}");
    assert!(imports.contains(&("safe", Some("cjson"))), "{imports:?}");
    assert!(
        imports.contains(&("config", Some("gateway"))),
        "{imports:?}"
    );
}
