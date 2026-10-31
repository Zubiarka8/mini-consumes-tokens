// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-lua/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_lua::LuaParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    LuaParser
        .parse(&SourceFile {
            relative_path: "module.lua".to_string(),
            contents: src.to_string(),
        })
        .expect("valid Lua source should parse")
}

#[test]
fn file_module_ends_on_the_last_line() {
    // A trailing newline must not push the module one line past the file.
    let module = |src: &str| {
        let parsed = parse(src);
        let m = parsed
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module && s.name == "module")
            .unwrap();
        (m.location.line, m.location.end_line)
    };
    let body = "local function f()\n  return 1\nend";
    assert_eq!(module(&format!("{body}\n")), (1, Some(3)));
    assert_eq!(module(body), (1, Some(3)));
    assert_eq!(module("f()\n"), (1, Some(1)));
}

#[test]
fn extracts_named_function_and_call() {
    let parsed = parse("function add(a, b)\n  return helper(a, b)\nend\n");
    let func = parsed
        .symbols
        .iter()
        .find(|s| s.name == "add")
        .expect("add should be indexed");
    assert_eq!(func.kind, SymbolKind::Function);

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"helper"));
}

#[test]
fn extracts_anonymous_function_assigned_to_local() {
    let parsed = parse("local square = function(x)\n  return x * x\nend\n");
    let func = parsed
        .symbols
        .iter()
        .find(|s| s.name == "square")
        .expect("square (anonymous function bound to a local) should be indexed");
    assert_eq!(func.kind, SymbolKind::Function);
}

#[test]
fn extracts_table_module_with_dotted_and_method_functions() {
    let parsed = parse(
        "local M = {}\n\nfunction M.new(x)\n  return x\nend\n\nfunction M:greet()\n  print('hi')\nend\n\nreturn M\n",
    );
    let module = parsed
        .symbols
        .iter()
        .find(|s| s.name == "M")
        .expect("M table should be indexed as a module");
    assert_eq!(module.kind, SymbolKind::Module);

    let new_fn = parsed
        .symbols
        .iter()
        .find(|s| s.name == "new")
        .expect("M.new should be indexed");
    assert_eq!(new_fn.kind, SymbolKind::Method);
    assert_eq!(new_fn.parent.as_deref(), Some("M"));

    let greet_fn = parsed
        .symbols
        .iter()
        .find(|s| s.name == "greet")
        .expect("M:greet should be indexed");
    assert_eq!(greet_fn.kind, SymbolKind::Method);
    assert_eq!(greet_fn.parent.as_deref(), Some("M"));

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"print"));
}

#[test]
fn extracts_require_as_import() {
    let parsed = parse("local other = require('other_module')\n");
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"other_module"));
}

#[test]
fn member_named_require_is_a_call_not_a_module_import() {
    let parsed = parse("loader.require('pkg')\nloader:require('other')\nrequire('actual')\n");
    assert!(parsed
        .relations
        .iter()
        .any(|r| { r.kind == mct_core::RelationKind::Imports && r.to_name == "actual" }));
    assert!(
        !parsed.relations.iter().any(|r| {
            r.kind == mct_core::RelationKind::Imports
                && ["pkg", "other"].contains(&r.to_name.as_str())
        }),
        "{:?}",
        parsed.relations
    );
    assert_eq!(
        parsed
            .relations
            .iter()
            .filter(|r| { r.kind == mct_core::RelationKind::Calls && r.to_name == "require" })
            .count(),
        2
    );
}

#[test]
fn file_scope_values_and_named_table_fields_are_indexed() {
    let parsed = parse("local limit = 5\nlocal Settings = { retries = 3, enabled = true }\nfunction f() local temporary = 1 end\n");
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == "limit" && s.kind == SymbolKind::Variable));
    for name in ["retries", "enabled"] {
        assert!(
            parsed.symbols.iter().any(|s| s.name == name
                && s.kind == SymbolKind::Field
                && s.parent.as_deref() == Some("Settings")),
            "{name}: {:?}",
            parsed.symbols
        );
    }
    assert!(
        parsed.symbols.iter().any(|s| s.name == "temporary"
            && s.kind == SymbolKind::Variable
            && s.parent.as_deref() == Some("f")),
        "{:?}",
        parsed.symbols
    );
}

#[test]
fn lua_const_attribute_is_indexed_as_a_constant() {
    let parsed = parse("local limit <const> = 5\n");
    assert!(
        parsed
            .symbols
            .iter()
            .any(|s| s.name == "limit" && s.kind == SymbolKind::Constant),
        "{:?}",
        parsed.symbols
    );
}

#[test]
fn negative_literal_values_are_indexed() {
    let parsed = parse("local low = -5\nlocal Config = { offset = -1 }\n");
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == "low" && s.kind == SymbolKind::Variable));
    assert!(parsed.symbols.iter().any(|s| s.name == "offset"
        && s.kind == SymbolKind::Field
        && s.parent.as_deref() == Some("Config")));
}

#[test]
fn extracts_nested_dotted_function_declaration() {
    let parsed = parse("function Foo.Bar.baz()\nend\n");
    let baz = parsed
        .symbols
        .iter()
        .find(|s| s.name == "baz")
        .expect("Foo.Bar.baz should be indexed");
    assert_eq!(baz.parent.as_deref(), Some("Foo.Bar"));
}

fn symbol<'a>(parsed: &'a mct_core::ParsedFile, name: &str) -> &'a mct_core::SymbolRecord {
    parsed
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol {name}"))
}

#[test]
fn a_function_assigned_to_a_target_spans_its_body() {
    let parsed = parse("local Money = {}\nMoney.__add = function(a, b)\n  return a:plus(b)\nend\n");
    let add = symbol(&parsed, "__add");
    assert_eq!((add.location.line, add.location.end_line), (2, Some(4)));
    // `T.f = function` is a method of T, like `function T.f()`.
    assert_eq!(
        (add.kind, add.parent.as_deref()),
        (SymbolKind::Method, Some("Money"))
    );
}

#[test]
fn only_top_level_named_tables_are_modules() {
    let parsed = parse(
        "local M = {}\nM.cache = {}\nfunction M.route(self, p)\n  local out = { p }\n  self.routes[#self.routes + 1] = { p }\n  return out\nend\nreturn M\n",
    );
    let modules: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Module)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(modules, ["module", "M", "cache"]);
}

#[test]
fn functions_in_a_table_constructor_are_methods_of_the_table() {
    let parsed = parse(
        "local handlers = {\n  get_stock = function(p)\n    return check(p)\n  end,\n  limit = 10,\n}\n",
    );
    let f = symbol(&parsed, "get_stock");
    assert_eq!(
        (
            f.kind,
            f.parent.as_deref(),
            f.location.line,
            f.location.end_line
        ),
        (SymbolKind::Method, Some("handlers"), 2, Some(4))
    );
    let call = parsed
        .relations
        .iter()
        .find(|r| r.to_name == "check")
        .unwrap();
    assert_eq!(call.from, f.id);
}

#[test]
fn calls_in_the_called_expression_are_extracted() {
    let parsed = parse("local function show(net)\n  return money.gross(net, 21):to_json()\nend\n");
    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert_eq!(calls, ["to_json", "gross"]);
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = LuaParser.parse(&SourceFile {
        relative_path: "broken.lua".to_string(),
        contents: "function broken(\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}

fn parse_at(path: &str, src: &str) -> mct_core::ParsedFile {
    LuaParser
        .parse(&SourceFile {
            relative_path: path.to_string(),
            contents: src.to_string(),
        })
        .expect("valid Lua source should parse")
}

#[test]
fn a_package_init_file_is_named_after_its_directory() {
    let module = |path: &str| {
        parse_at(path, "return {}\n")
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module)
            .map(|s| s.name.clone())
            .unwrap()
    };
    // `require("app.shapes")` loads `app/shapes/init.lua`.
    assert_eq!(module("app/shapes/init.lua"), "shapes");
    assert_eq!(module("shapes.lua"), "shapes");
    // No directory to name it after.
    assert_eq!(module("init.lua"), "init");
}

#[test]
fn dofile_and_loadfile_paths_are_unresolved_because_the_working_directory_is_unknown() {
    let parsed = parse(
        "dofile('lib/util.lua')\nlocal chunk = loadfile(\"./lib/helper.lua\")\n\
         dofile(computed)\nloader.dofile('skipped.lua')\ndofile('config')\ndofile('/abs/x.lua')\n",
    );
    let relation = |kind: RelationKind, to: &str| {
        parsed
            .relations
            .iter()
            .position(|r| r.kind == kind && r.to_name == to)
    };
    // The literal is kept exactly as written; nothing pins it to a file, so
    // no target evidence is attached and the index leaves it unresolved.
    for (kind, path) in [
        (RelationKind::Imports, "lib/util.lua"),
        (RelationKind::References, "./lib/helper.lua"),
        (RelationKind::Imports, "config"),
        (RelationKind::Imports, "/abs/x.lua"),
    ] {
        let at = relation(kind, path).unwrap_or_else(|| panic!("{kind:?} {path}"));
        assert!(
            !parsed.relation_targets.iter().any(|t| t.relation == at),
            "{path} must stay unresolved"
        );
    }
    // A computed path and a member call are plain calls, nothing else.
    assert!(relation(RelationKind::Imports, "skipped.lua").is_none());
    assert_eq!(
        parsed
            .relations
            .iter()
            .filter(|r| r.kind == RelationKind::Calls && r.to_name == "dofile")
            .count(),
        2
    );
}

#[test]
fn computed_file_scope_assignments_are_variables() {
    let parsed = parse(
        "local total = compute(1)\nlocal sum = a + b\nlocal json = require('cjson')\n\
         name = os.getenv('NAME')\nlocal first, second = pair()\nlocal Cfg = {}\nCfg.path = base .. '/x'\n",
    );
    for name in ["total", "sum", "name", "first", "second"] {
        assert!(
            parsed
                .symbols
                .iter()
                .any(|s| s.name == name && s.kind == SymbolKind::Variable && s.parent.is_none()),
            "{name}: {:?}",
            parsed.symbols
        );
    }
    assert!(parsed.symbols.iter().any(|s| s.name == "path"
        && s.kind == SymbolKind::Field
        && s.parent.as_deref() == Some("Cfg")));
    // A name bound to `require` aliases the module: an import, not a symbol.
    assert!(!parsed.symbols.iter().any(|s| s.name == "json"));
    // The call and the import are still relations, not only symbols.
    assert!(parsed
        .relations
        .iter()
        .any(|r| r.kind == RelationKind::Calls && r.to_name == "compute"));
    assert!(parsed
        .relations
        .iter()
        .any(|r| r.kind == RelationKind::Imports && r.to_name == "cjson"));
}

#[test]
fn function_locals_are_variables_of_their_function_and_globals_are_not_its_children() {
    let parsed = parse(
        "function run()\n  local a = 1\n  local b = compute()\n  local c = {}\n  leaked = 2\n\
           if a then local d = b end\n  for i = 1, 3 do local e = i end\nend\n\
         function T.m() local inner = 1 end\n",
    );
    for name in ["a", "b", "c", "d", "e"] {
        assert!(
            parsed.symbols.iter().any(|s| s.name == name
                && s.kind == SymbolKind::Variable
                && s.parent.as_deref() == Some("run")),
            "{name}: {:?}",
            parsed.symbols
        );
    }
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == "inner" && s.parent.as_deref() == Some("m")));
    // A global assigned inside a function is a global, not a local of it.
    let leaked = symbol(&parsed, "leaked");
    assert_eq!(leaked.kind, SymbolKind::Variable);
    assert_eq!(leaked.parent, None);
    // The numeric loop variable belongs to the function too.
    assert!(parsed.symbols.iter().any(|s| s.name == "i"
        && s.kind == SymbolKind::Variable
        && s.parent.as_deref() == Some("run")));
}

#[test]
fn tables_with_metatables_are_classes_with_inheritance() {
    let parsed = parse(
        "local Animal = {}\nAnimal.__index = Animal\n\
         function Animal.new(name) return setmetatable({ name = name }, Animal) end\n\
         local Dog = setmetatable({}, { __index = Animal })\nDog.__index = Dog\n\
         local Cat = {}\nsetmetatable(Cat, { __index = Animal })\n\
         local Bird = setmetatable({}, Animal)\n\
         local Plain = {}\nPlain.value = 1\n",
    );
    let kind_of = |name: &str| symbol(&parsed, name).kind;
    for class in ["Animal", "Dog", "Cat", "Bird"] {
        assert_eq!(kind_of(class), SymbolKind::Class, "{class}");
    }
    assert_eq!(kind_of("Plain"), SymbolKind::Module);
    let extends = |class: &str| -> Vec<&str> {
        let id = symbol(&parsed, class).id;
        parsed
            .relations
            .iter()
            .filter(|r| r.kind == RelationKind::Extends && r.from == id)
            .map(|r| r.to_name.as_str())
            .collect()
    };
    for child in ["Dog", "Cat", "Bird"] {
        assert_eq!(extends(child), ["Animal"], "{child}");
    }
    assert!(extends("Animal").is_empty());
    // The metamethod slot is not a field of its own.
    assert!(!parsed.symbols.iter().any(|s| s.name == "__index"));
}

fn variables<'a>(parsed: &'a mct_core::ParsedFile, name: &str) -> Vec<&'a mct_core::SymbolRecord> {
    parsed.symbols.iter().filter(|s| s.name == name).collect()
}

#[test]
fn file_scope_globals_are_indexed_once_however_often_they_are_assigned() {
    let parsed = parse("GLOBAL_CONFIG = {}\nCOUNT = 1\nCOUNT = 2\nlocal n = 1\nn = 2\n");
    assert_eq!(variables(&parsed, "COUNT").len(), 1);
    assert_eq!(variables(&parsed, "n").len(), 1);
    assert_eq!(symbol(&parsed, "GLOBAL_CONFIG").kind, SymbolKind::Module);
}

#[test]
fn a_global_assigned_in_a_function_is_indexed_once_and_a_shadowing_local_is_not_a_global() {
    let parsed = parse(
        "local shared = 0\nfunction a() flag = true; shared = 1 end\n\
         function b() flag = false; local shared = 2 end\n",
    );
    let flags = variables(&parsed, "flag");
    assert_eq!(flags.len(), 1, "{flags:?}");
    assert_eq!(flags[0].parent, None);
    // `shared = 1` reassigns the file-level local; the one in `b` is a new
    // local of `b`.
    let shared = variables(&parsed, "shared");
    assert_eq!(shared.len(), 2, "{shared:?}");
    assert_eq!(shared.iter().filter(|s| s.parent.is_none()).count(), 1);
    assert_eq!(
        shared
            .iter()
            .filter(|s| s.parent.as_deref() == Some("b"))
            .count(),
        1
    );
}

#[test]
fn loop_variables_of_nested_scopes_belong_to_the_enclosing_function() {
    let parsed = parse(
        "local function process(items)\n  for i, value in ipairs(items) do\n    for j = 1, i do\n      local cell = value\n    end\n  end\nend\n",
    );
    for name in ["i", "value", "j", "cell"] {
        let found = variables(&parsed, name);
        assert_eq!(found.len(), 1, "{name}: {found:?}");
        assert_eq!(found[0].kind, SymbolKind::Variable);
        assert_eq!(found[0].parent.as_deref(), Some("process"), "{name}");
    }
    // Parameters are scope only, not symbols.
    assert!(variables(&parsed, "items").is_empty());
}

#[test]
fn a_loop_variable_shadows_a_global_so_assigning_it_declares_nothing_new() {
    let parsed = parse("function f()\n  for i = 1, 3 do\n    i = i + 1\n  end\nend\n");
    assert_eq!(variables(&parsed, "i").len(), 1);
}

#[test]
fn a_bare_local_declaration_inside_a_function_is_a_variable() {
    let parsed = parse("function f()\n  local result\n  result = 1\nend\n");
    let found = variables(&parsed, "result");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].parent.as_deref(), Some("f"));
}

#[test]
fn a_forward_declared_file_local_is_indexed_by_its_defining_assignment() {
    let parsed = parse("local M\nM = {}\nfunction M.run() end\n");
    let found = variables(&parsed, "M");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].kind, SymbolKind::Module);
}

#[test]
fn table_keys_that_are_not_identifiers_are_fields_with_their_source_text() {
    let parsed = parse(
        "local config = {\n  [\"api-key\"] = \"secret\",\n  [10] = \"value\",\n  [prefix .. \"x\"] = 1,\n  plain = 2,\n  \"positional\",\n  [\"handler\"] = function() end,\n}\n",
    );
    let field = |name: &str| {
        let f = parsed
            .symbols
            .iter()
            .find(|s| s.name == name && s.parent.as_deref() == Some("config"))
            .unwrap_or_else(|| panic!("{name}: {:?}", parsed.symbols));
        f.kind
    };
    assert_eq!(field("api-key"), SymbolKind::Field);
    assert_eq!(field("[10]"), SymbolKind::Field);
    assert_eq!(field("[prefix .. \"x\"]"), SymbolKind::Field);
    assert_eq!(field("plain"), SymbolKind::Field);
    assert_eq!(field("handler"), SymbolKind::Method);
    // A positional value has no key to index.
    assert_eq!(
        parsed
            .symbols
            .iter()
            .filter(|s| s.parent.as_deref() == Some("config"))
            .count(),
        5
    );
}

fn extends_of<'a>(parsed: &'a mct_core::ParsedFile, class: &str) -> Vec<(&'a str, usize)> {
    let id = symbol(parsed, class).id;
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Extends && r.from == id)
        .map(|(i, r)| (r.to_name.as_str(), i))
        .collect()
}

fn target_for(parsed: &mct_core::ParsedFile, relation: usize) -> Option<&mct_core::RelationTarget> {
    parsed
        .relation_targets
        .iter()
        .find(|t| t.relation == relation)
}

#[test]
fn a_base_reached_through_require_extends_the_module_it_was_required_from() {
    let parsed = parse(
        "local Base = require(\"core.base\")\nlocal Child = setmetatable({}, { __index = Base })\n\
         local Alias = Base\nlocal Grand = setmetatable({}, { __index = Alias })\n",
    );
    for class in ["Child", "Grand"] {
        let extends = extends_of(&parsed, class);
        assert_eq!(extends.len(), 1, "{class}");
        // The relation names the required module, not the local alias.
        assert_eq!(extends[0].0, "base", "{class}");
        let target = target_for(&parsed, extends[0].1).expect("evidence");
        assert_eq!(target.module.as_deref(), Some("core"));
        assert_eq!(target.target_module.as_deref(), Some("base"));
        assert_eq!(target.kind, Some(SymbolKind::Module));
    }
}

#[test]
fn a_base_taken_from_a_required_module_field_is_named_by_that_field() {
    let parsed = parse(
        "local shapes = require(\"app.shapes\")\nlocal Square = setmetatable({}, { __index = shapes.Rect })\n\
         local Circle = setmetatable({}, { __index = require(\"app.shapes\").Round })\n",
    );
    for (class, base) in [("Square", "Rect"), ("Circle", "Round")] {
        let extends = extends_of(&parsed, class);
        assert_eq!(extends.len(), 1, "{class}");
        assert_eq!(extends[0].0, base);
        let target = target_for(&parsed, extends[0].1).expect("evidence");
        assert_eq!(target.target_module.as_deref(), Some("shapes"));
        assert_eq!(target.module.as_deref(), Some("app"));
    }
}

#[test]
fn a_shadowed_import_alias_is_not_followed() {
    let parsed = parse(
        "local Base = require(\"core.base\")\nlocal function make()\n  local Base = {}\n  return Base\nend\n\
         local Child = setmetatable({}, { __index = Base })\n",
    );
    assert_eq!(extends_of(&parsed, "Child")[0].0, "base");
    let local = parse("local Base = {}\nlocal Child = setmetatable({}, { __index = Base })\n");
    let extends = extends_of(&local, "Child");
    assert_eq!(extends[0].0, "Base");
    assert!(target_for(&local, extends[0].1).is_none());
}

#[test]
fn class_helpers_declare_classes_with_their_explicit_base() {
    let parsed = parse(
        "local class = require(\"middleclass\")\nlocal Fruit = class('Fruit')\n\
         local Apple = class('Apple', Fruit)\nlocal Base = require(\"core.base\")\n\
         local Pear = class('Pear', Base)\nlocal Plum = Fruit:subclass('Plum')\n\
         local Cherry = Base:extend()\nlocal Other = thing:extend()\n",
    );
    assert_eq!(symbol(&parsed, "Fruit").kind, SymbolKind::Class);
    assert!(extends_of(&parsed, "Fruit").is_empty());
    assert_eq!(extends_of(&parsed, "Apple")[0].0, "Fruit");
    assert_eq!(extends_of(&parsed, "Plum")[0].0, "Fruit");
    let pear = extends_of(&parsed, "Pear");
    assert_eq!(pear[0].0, "base");
    assert_eq!(
        target_for(&parsed, pear[0].1)
            .unwrap()
            .target_module
            .as_deref(),
        Some("base")
    );
    assert_eq!(extends_of(&parsed, "Cherry")[0].0, "base");
    // `thing` is neither an import nor a class of this file: not a class.
    assert!(parsed
        .symbols
        .iter()
        .all(|s| s.name != "Other" || s.kind != SymbolKind::Class));
}

#[test]
fn a_locally_defined_class_function_is_not_a_class_library() {
    let parsed = parse("local function class(x) return x end\nlocal Thing = class('Thing')\n");
    assert!(parsed
        .symbols
        .iter()
        .all(|s| s.name != "Thing" || s.kind != SymbolKind::Class));
}

#[test]
fn a_functional_index_delegating_to_a_table_is_a_reference_not_an_extension() {
    let parsed = parse(
        "local Base = {}\nlocal Proxy = {}\n\
         setmetatable(Proxy, { __index = function(t, k) return Base[k] end })\n\
         local Dynamic = {}\n\
         setmetatable(Dynamic, { __index = function(t, k) return compute(k) end })\n",
    );
    let proxy = symbol(&parsed, "Proxy");
    assert!(parsed
        .relations
        .iter()
        .any(|r| r.from == proxy.id && r.kind == RelationKind::References && r.to_name == "Base"));
    assert!(extends_of(&parsed, "Proxy").is_empty());
    // A computed lookup proves no delegation target: no relation of any kind.
    let dynamic = symbol(&parsed, "Dynamic");
    assert!(!parsed.relations.iter().any(|r| r.from == dynamic.id
        && matches!(r.kind, RelationKind::Extends | RelationKind::References)));
}
