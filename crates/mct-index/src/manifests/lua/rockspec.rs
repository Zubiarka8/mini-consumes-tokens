//! Static analysis of one `.rockspec`.
//!
//! A rockspec is Lua code. The analysis never runs it: [`super::data`] folds
//! exact constants and stops at anything else. A section is reported
//! `complete` only when the whole file was folded, because any statement the
//! reader could not fold may rewrite any section. One unrecognised element
//! inside a fully folded file makes only its own section `incomplete`.

use std::collections::BTreeMap;

use super::data::{self, Table, Value};
use super::version;
use crate::manifests::ManifestDependency;

pub(crate) const SECTION_DEPENDENCIES: &str = "dependencies";
pub(crate) const SECTION_BUILD_DEPENDENCIES: &str = "build_dependencies";
pub(crate) const SECTION_TEST_DEPENDENCIES: &str = "test_dependencies";
pub(crate) const SECTION_MODULES: &str = "build.modules";
pub(crate) const SECTION_INSTALL: &str = "build.install.lua";
pub(crate) const SECTION_PLATFORMS: &str = "platforms";

/// Every section, in storage order.
pub(crate) const SECTIONS: [&str; 6] = [
    SECTION_DEPENDENCIES,
    SECTION_BUILD_DEPENDENCIES,
    SECTION_TEST_DEPENDENCIES,
    SECTION_MODULES,
    SECTION_INSTALL,
    SECTION_PLATFORMS,
];

/// The sections that declare which rocks a project needs.
pub(crate) const DECLARATION_SECTIONS: [&str; 4] = [
    SECTION_DEPENDENCIES,
    SECTION_BUILD_DEPENDENCIES,
    SECTION_TEST_DEPENDENCIES,
    SECTION_PLATFORMS,
];

pub(crate) const STATE_COMPLETE: &str = "complete";
pub(crate) const STATE_INCOMPLETE: &str = "incomplete";
pub(crate) const STATE_UNREADABLE: &str = "unreadable";

pub(crate) const KIND_FILE: &str = "file";
pub(crate) const KIND_SECTION: &str = "section";
pub(crate) const KIND_DEPENDENCY: &str = "dependency";
pub(crate) const KIND_MODULE: &str = "module";

/// One row of `rockspec_items`.
///
/// - `file`: `name` is the package, `constraint` the rockspec version,
///   `detail` the `rockspec_format` (or the reason when `unreadable`).
/// - `section`: `name` is the section, `detail` why it is `incomplete`.
/// - `dependency`: `scope`, `namespace`, `name`, `constraint`, `platform`.
/// - `module`: `name` is the module, `path` its file (the first source of a
///   native module), `native`, and `source` the table it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub kind: &'static str,
    pub section: Option<&'static str>,
    pub scope: Option<&'static str>,
    pub platform: Option<String>,
    pub namespace: Option<String>,
    pub name: String,
    pub constraint: Option<String>,
    pub path: Option<String>,
    pub native: bool,
    pub source: Option<&'static str>,
    pub state: &'static str,
    pub detail: Option<String>,
}

impl Item {
    fn new(kind: &'static str, name: String) -> Self {
        Self {
            kind,
            section: None,
            scope: None,
            platform: None,
            namespace: None,
            name,
            constraint: None,
            path: None,
            native: false,
            source: None,
            state: STATE_COMPLETE,
            detail: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Analysis {
    pub items: Vec<Item>,
}

impl Analysis {
    /// The rockspec could not be analysed at all (not UTF-8).
    pub(crate) fn unreadable(reason: &str) -> Self {
        let mut file = Item::new(KIND_FILE, String::new());
        file.state = STATE_UNREADABLE;
        file.detail = Some(reason.to_string());
        Self { items: vec![file] }
    }

    /// The name/version summary stored in `dependencies`: top-level entries
    /// first, then platform entries; the `lua` interpreter is not a package.
    pub(crate) fn projection(&self) -> Vec<ManifestDependency> {
        let mut top: Vec<&Item> = Vec::new();
        let mut platform: Vec<&Item> = Vec::new();
        for item in self.items.iter().filter(|i| i.kind == KIND_DEPENDENCY) {
            if item.namespace.is_none() && item.name == "lua" {
                continue;
            }
            if item.platform.is_some() {
                platform.push(item);
            } else {
                top.push(item);
            }
        }
        top.into_iter()
            .chain(platform)
            .map(|item| ManifestDependency {
                name: match &item.namespace {
                    Some(ns) => format!("{ns}/{}", item.name),
                    None => item.name.clone(),
                },
                version: item.constraint.clone(),
            })
            .collect()
    }
}

struct Builder {
    items: Vec<Item>,
    incomplete: BTreeMap<&'static str, String>,
}

impl Builder {
    fn mark(&mut self, section: &'static str, reason: String) {
        self.incomplete.entry(section).or_insert(reason);
    }
}

/// Analyses the text of a rockspec.
pub(crate) fn analyse(source: &str) -> Analysis {
    let chunk = data::read(source);
    let mut builder = Builder {
        items: Vec::new(),
        incomplete: BTreeMap::new(),
    };
    let env = &chunk.env;

    for (key, scope, section) in [
        (SECTION_DEPENDENCIES, "runtime", SECTION_DEPENDENCIES),
        (
            SECTION_BUILD_DEPENDENCIES,
            "build",
            SECTION_BUILD_DEPENDENCIES,
        ),
        (SECTION_TEST_DEPENDENCIES, "test", SECTION_TEST_DEPENDENCIES),
    ] {
        dependency_section(env.get(key), scope, section, None, &mut builder);
    }
    if let Some(build) = env.get("build") {
        build_tables(build, None, SECTION_MODULES, SECTION_INSTALL, &mut builder);
    }
    if let Some(platforms) = env.get("platforms") {
        match platforms {
            Value::Table(table) => {
                if table.positional().next().is_some() || table.has_other_keys() {
                    builder.mark(
                        SECTION_PLATFORMS,
                        "platforms is not a map of platform names".into(),
                    );
                }
                for (platform, value) in table.named() {
                    platform_tables(platform, value, &mut builder);
                }
            }
            Value::Nil => {}
            _ => builder.mark(SECTION_PLATFORMS, "platforms is not a table".into()),
        }
    }

    if let Some(stop) = &chunk.stop {
        let reason = format!("{} (line {})", stop.reason, stop.line);
        for section in SECTIONS {
            builder.mark(section, reason.clone());
        }
    }

    let text = |name: &str| env.get(name).and_then(Value::as_str).map(str::to_string);
    let mut file = Item::new(KIND_FILE, text("package").unwrap_or_default());
    file.constraint = text("version");
    file.detail = text("rockspec_format");
    if chunk.stop.is_some() {
        file.state = STATE_INCOMPLETE;
    }
    let mut items = vec![file];
    for section in SECTIONS {
        let mut row = Item::new(KIND_SECTION, section.to_string());
        if let Some(reason) = builder.incomplete.get(section) {
            row.state = STATE_INCOMPLETE;
            row.detail = Some(reason.clone());
        }
        items.push(row);
    }
    items.append(&mut builder.items);
    Analysis { items }
}

fn platform_tables(platform: &str, value: &Value, builder: &mut Builder) {
    let Value::Table(table) = value else {
        builder.mark(
            SECTION_PLATFORMS,
            format!("platform `{platform}` is not a table"),
        );
        return;
    };
    for (key, scope) in [
        (SECTION_DEPENDENCIES, "runtime"),
        (SECTION_BUILD_DEPENDENCIES, "build"),
        (SECTION_TEST_DEPENDENCIES, "test"),
    ] {
        dependency_section(
            table.get(key),
            scope,
            SECTION_PLATFORMS,
            Some(platform),
            builder,
        );
    }
    if let Some(build) = table.get("build") {
        build_tables(
            build,
            Some(platform),
            SECTION_PLATFORMS,
            SECTION_PLATFORMS,
            builder,
        );
    }
}

fn build_tables(
    build: &Value,
    platform: Option<&str>,
    modules_section: &'static str,
    install_section: &'static str,
    builder: &mut Builder,
) {
    let table = match build {
        Value::Table(table) => table,
        Value::Nil => return,
        _ => {
            builder.mark(modules_section, "build is not a table".into());
            builder.mark(install_section, "build is not a table".into());
            return;
        }
    };
    if let Some(modules) = table.get("modules") {
        module_table(modules, modules_section, "build.modules", platform, builder);
    }
    match table.get("install") {
        Some(Value::Table(install)) => {
            if let Some(lua) = install.get("lua") {
                module_table(lua, install_section, "build.install.lua", platform, builder);
            }
        }
        Some(Value::Nil) | None => {}
        Some(_) => builder.mark(install_section, "build.install is not a table".into()),
    }
}

fn dependency_section(
    value: Option<&Value>,
    scope: &'static str,
    section: &'static str,
    platform: Option<&str>,
    builder: &mut Builder,
) {
    let table = match value {
        None | Some(Value::Nil) => return,
        Some(Value::Table(table)) => table,
        Some(_) => {
            builder.mark(section, format!("{scope} dependencies are not a table"));
            return;
        }
    };
    if !table.named().is_empty() || table.has_other_keys() {
        builder.mark(section, "dependencies table has non-list entries".into());
    }
    for entry in table.positional() {
        let Some(text) = entry.as_str() else {
            builder.mark(section, "a dependency entry is not a string".into());
            continue;
        };
        match parse_dependency(text) {
            Ok(parsed) => {
                let mut item = Item::new(KIND_DEPENDENCY, parsed.name);
                item.section = Some(section);
                item.scope = Some(scope);
                item.platform = platform.map(str::to_string);
                item.namespace = parsed.namespace;
                if let Some(constraint) = parsed.constraint {
                    if version::parse_constraints(&constraint).is_none() {
                        item.state = STATE_INCOMPLETE;
                        item.detail = Some("constraint not recognised".into());
                        builder.mark(section, format!("constraint `{constraint}` not recognised"));
                    }
                    item.constraint = Some(constraint);
                }
                builder.items.push(item);
            }
            Err(reason) => builder.mark(section, reason),
        }
    }
}

struct ParsedDependency {
    namespace: Option<String>,
    name: String,
    constraint: Option<String>,
}

/// `queries.from_dep_string`: optional `namespace/`, a lowercased name, then
/// the constraint text.
fn parse_dependency(text: &str) -> Result<ParsedDependency, String> {
    let trimmed = text.trim_start();
    let len = trimmed
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'/'))
        .count();
    let ns_name = &trimmed[..len];
    let rest = trimmed[len..].trim();
    let (namespace, name) = match ns_name.split_once('/') {
        Some((ns, name)) => (Some(ns), name),
        None => (None, ns_name),
    };
    let name_ok = name
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphanumeric())
        && !name.contains('/')
        && namespace.is_none_or(|ns| !ns.is_empty());
    if !name_ok || (namespace.is_none() && !ns_name.bytes().any(|b| b.is_ascii_alphanumeric())) {
        return Err(format!(
            "dependency `{}` has no recognisable name",
            text.trim()
        ));
    }
    if rest.contains('/') {
        return Err(format!(
            "dependency `{}` has a `/` in its constraint",
            text.trim()
        ));
    }
    Ok(ParsedDependency {
        namespace: namespace.map(str::to_lowercase),
        name: name.to_lowercase(),
        constraint: (!rest.is_empty()).then(|| rest.to_string()),
    })
}

fn module_table(
    value: &Value,
    section: &'static str,
    source: &'static str,
    platform: Option<&str>,
    builder: &mut Builder,
) {
    let table: &Table = match value {
        Value::Table(table) => table,
        Value::Nil => return,
        _ => {
            builder.mark(section, format!("{source} is not a table"));
            return;
        }
    };
    if table.positional().next().is_some() || table.has_other_keys() {
        builder.mark(
            section,
            format!("{source} has entries that are not module names"),
        );
    }
    for (name, entry) in table.named() {
        let mut item = Item::new(KIND_MODULE, name.to_string());
        item.section = Some(section);
        item.platform = platform.map(str::to_string);
        item.source = Some(source);
        match entry {
            Value::Str(path) => item.path = Some(path.clone()),
            Value::Table(sources) => {
                item.native = true;
                item.path = sources
                    .positional()
                    .find_map(|v| v.as_str().map(str::to_string))
                    .or_else(|| {
                        sources.get("sources").and_then(|s| match s {
                            Value::Str(one) => Some(one.clone()),
                            Value::Table(list) => list
                                .positional()
                                .find_map(|v| v.as_str().map(str::to_string)),
                            _ => None,
                        })
                    });
            }
            _ => {
                builder.mark(
                    section,
                    format!("module `{name}` has an unrecognised value"),
                );
                continue;
            }
        }
        builder.items.push(item);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn state(analysis: &Analysis, section: &str) -> &'static str {
        analysis
            .items
            .iter()
            .find(|i| i.kind == KIND_SECTION && i.name == section)
            .unwrap()
            .state
    }

    #[test]
    fn a_fully_folded_rockspec_is_complete_in_every_section() {
        let a = analyse(
            "package = 'Demo'\nversion = '1.0-1'\nrockspec_format = '3.0'\n\
             dependencies = { 'lua >= 5.1', 'LuaSocket ~> 3.0', 'user/Thing', }\n\
             build_dependencies = { 'luarocks-build-rust-mlua >= 0.2' }\n\
             build = { type = 'builtin', modules = { demo = 'src/demo.lua', ['demo.fast'] = { 'a.c', 'b.c' } } }\n",
        );
        for section in SECTIONS {
            assert_eq!(state(&a, section), STATE_COMPLETE, "{section}");
        }
        let deps: Vec<_> = a
            .items
            .iter()
            .filter(|i| i.kind == KIND_DEPENDENCY)
            .collect();
        assert_eq!(deps.len(), 4);
        assert_eq!(deps[1].name, "luasocket");
        assert_eq!(deps[1].constraint.as_deref(), Some("~> 3.0"));
        assert_eq!(deps[2].namespace.as_deref(), Some("user"));
        assert_eq!(deps[3].scope, Some("build"));
        let modules: Vec<_> = a.items.iter().filter(|i| i.kind == KIND_MODULE).collect();
        assert_eq!(modules.len(), 2);
        assert!(modules[1].native && modules[1].path.as_deref() == Some("a.c"));
        assert_eq!(a.projection().len(), 3, "`lua` is not a package");
    }

    #[test]
    fn the_word_dependencies_inside_text_is_not_a_declaration() {
        let a = analyse("description = { detailed = [[\ndependencies = { 'evil' }\n]] }\n");
        assert!(a.items.iter().all(|i| i.kind != KIND_DEPENDENCY));
        assert_eq!(state(&a, SECTION_DEPENDENCIES), STATE_COMPLETE);
    }

    #[test]
    fn any_unfoldable_code_makes_every_section_incomplete_but_keeps_what_was_read() {
        for source in [
            "dependencies = { 'a' }\nif os.getenv('X') then dependencies[#dependencies + 1] = 'b' end\n",
            "dependencies = { 'a' }\ndescription = { summary = tostring(1) }\n",
            "dependencies = { 'a' }\nlocal helper = require('h')\n",
            "dependencies = { 'a' }\nbuild = { modules = unknown_table }\n",
            "dependencies = { 'a'\n",
        ] {
            let a = analyse(source);
            for section in SECTIONS {
                assert_eq!(state(&a, section), STATE_INCOMPLETE, "{section}: {source}");
            }
            assert!(
                a.items.iter().any(|i| i.kind == KIND_DEPENDENCY && i.name == "a")
                    || source.ends_with("'a'\n"),
                "{source}"
            );
        }
    }

    #[test]
    fn one_unrecognised_element_only_taints_its_own_section() {
        let a = analyse(
            "dependencies = { 'lpeg >>> 1' }\ntest_dependencies = { 'busted' }\n\
             build = { modules = { x = 1 } }\n",
        );
        assert_eq!(state(&a, SECTION_DEPENDENCIES), STATE_INCOMPLETE);
        assert_eq!(state(&a, SECTION_MODULES), STATE_INCOMPLETE);
        assert_eq!(state(&a, SECTION_TEST_DEPENDENCIES), STATE_COMPLETE);
        let bad = a.items.iter().find(|i| i.name == "lpeg").unwrap();
        assert_eq!(bad.state, STATE_INCOMPLETE);
    }

    #[test]
    fn platform_entries_are_tagged_and_kept_apart() {
        let a = analyse(
            "dependencies = { 'a' }\nplatforms = { unix = { dependencies = { 'b' }, build = { modules = { m = 'm.lua' } } } }\n",
        );
        let b = a.items.iter().find(|i| i.name == "b").unwrap();
        assert_eq!(b.platform.as_deref(), Some("unix"));
        let m = a.items.iter().find(|i| i.kind == KIND_MODULE).unwrap();
        assert_eq!(m.platform.as_deref(), Some("unix"));
        assert_eq!(state(&a, SECTION_PLATFORMS), STATE_COMPLETE);
    }

    #[test]
    fn install_lua_is_recorded_with_its_own_source() {
        let a = analyse("build = { install = { lua = { ['demo.pkg'] = 'src/pkg/init.lua' } } }\n");
        let m = a.items.iter().find(|i| i.kind == KIND_MODULE).unwrap();
        assert_eq!(m.source, Some("build.install.lua"));
        assert_eq!(m.section, Some(SECTION_INSTALL));
    }

    #[test]
    fn repeated_names_and_constraints_are_all_kept() {
        let a = analyse("dependencies = { 'luasocket >= 3', 'luasocket < 4' }\ntest_dependencies = { 'luasocket' }\n");
        let n = a.items.iter().filter(|i| i.name == "luasocket").count();
        assert_eq!(n, 3);
    }
}
