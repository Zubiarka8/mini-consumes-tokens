//! An instance built by `setmetatable({…}, T)` (or `{ __index = T }`), where
//! `T` is a class of this file, carries fields of `T`: each named key of the
//! table it is built from, and each `x.f = …` through the local it is bound
//! to (`local self = setmetatable({}, T)`), also through a local alias of
//! the global (`local setmetatable = setmetatable`). A colon call on that
//! local (`self:m()`) is qualified by `T`, and so is a dot call
//! (`self.loader()`) when `T` declares that name in this file. A metatable that is no class of this
//! file, a computed key, and a local rebound to anything else declare
//! nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_lua::LuaParser;

const SOURCE: &str = r#"local Cache = {}
Cache.__index = Cache
local Meta = {}

function Cache.new(dict, ttl)
  local self = setmetatable({}, Cache)
  self.dict = dict
  self.ttl = ttl
  self.dict = nil
  self:reset()
  self.ttl()
  self.unknown()
  return self
end

function Cache.from(dict)
  return setmetatable({ dict = dict, hits = 0, ["x-y"] = 1 }, { __index = Cache })
end

function Cache:reset()
  self.misses = 0
end

local function other()
  local o = setmetatable({}, Meta)
  o.size = 1
  local p = setmetatable({ weight = 2 }, Meta)
  local q = setmetatable({}, Cache)
  q = {}
  q.lost = 1
end

return Cache
"#;

const ALIASED: &str = r#"local setmetatable = setmetatable
local Money = {}
Money.__index = Money
function Money.of(minor)
  return setmetatable({ minor = minor }, Money)
end
local function fake()
  local setmetatable = function(t) return t end
  return setmetatable({ cents = 1 }, Money)
end
return Money
"#;

#[test]
fn an_alias_of_the_global_builds_instances_and_a_local_function_does_not() {
    let parsed = mct_corpus::parse_source(&LuaParser, "money.lua", "Lua", ALIASED);
    let fields: Vec<(&str, Option<&str>, u32)> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Field)
        .map(|s| (s.name.as_str(), s.parent.as_deref(), s.location.line))
        .collect();
    assert_eq!(fields, [("minor", Some("Money"), 5)]);
}

#[test]
fn an_instance_of_a_class_carries_its_fields() {
    let parsed = mct_corpus::parse_source(&LuaParser, "cache.lua", "Lua", SOURCE);
    let fields: Vec<(&str, Option<&str>, u32)> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Field)
        .map(|s| (s.name.as_str(), s.parent.as_deref(), s.location.line))
        .collect();
    assert_eq!(
        fields,
        [
            ("dict", Some("Cache"), 7),
            ("ttl", Some("Cache"), 8),
            ("hits", Some("Cache"), 17),
            ("misses", Some("Cache"), 21),
        ]
    );
    let reset: Vec<Option<&str>> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls && r.to_name == "reset")
        .map(|(i, _)| {
            parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.qualifier.as_deref())
        })
        .collect();
    assert_eq!(reset, [Some("Cache")]);
    let qualifier_of = |name: &str| -> Vec<Option<&str>> {
        parsed
            .relations
            .iter()
            .enumerate()
            .filter(|(_, r)| r.kind == RelationKind::Calls && r.to_name == name)
            .map(|(i, _)| {
                parsed
                    .relation_targets
                    .iter()
                    .find(|t| t.relation == i)
                    .and_then(|t| t.qualifier.as_deref())
            })
            .collect()
    };
    assert_eq!(qualifier_of("ttl"), [Some("Cache")]);
    assert_eq!(qualifier_of("unknown"), [None]);
}
