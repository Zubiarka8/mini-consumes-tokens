//! `self.x = …` inside a colon method `function T:m()` assigns a field of
//! `T`'s instances, so its first assignment is a `field` of `T`; a later
//! assignment, or a name `T` already declares, adds nothing. `self` is
//! known in a colon method (and the closures inside it) and as a local built
//! by `setmetatable({}, T)` (see `instance_fields.rs`): an explicit `self`
//! parameter, a plain function or the file level declare no field.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::SymbolKind;
use mct_lang_lua::LuaParser;

const SOURCE: &str = r#"local Account = {}
Account.__index = Account
Account.limit = 10

function Account.new(owner)
  local self = setmetatable({}, Account)
  self.owner = owner
  return self
end

function Account:deposit(v)
  self.balance = (self.balance or 0) + v
  self.limit = 20
  local log = function()
    self.history = {}
  end
end

function Account:reset()
  self.balance = 0
end

function Account.close(self)
  self.closed = true
end

local function helper(self)
  self.tag = 1
end

return Account
"#;

#[test]
fn a_self_assignment_in_a_colon_method_is_a_field_of_its_table() {
    let parsed = mct_corpus::parse_source(&LuaParser, "src/account.lua", "Lua", SOURCE);
    let fields: Vec<(String, Option<String>, u32)> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Field)
        .map(|s| (s.name.clone(), s.parent.clone(), s.location.line))
        .collect();
    let field = |name: &str, line| (name.to_string(), Some("Account".to_string()), line);
    assert_eq!(
        fields,
        [
            field("limit", 3),
            field("owner", 7),
            field("balance", 12),
            field("history", 15)
        ]
    );
}
