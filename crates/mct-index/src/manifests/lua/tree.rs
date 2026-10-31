//! Parsers for the data LuaRocks writes next to a project: the `manifest` of
//! an installed tree store and `luarocks.lock`. Reading the files, and
//! deciding whether they may be read at all, is the caller's job.

use std::collections::BTreeMap;

use super::data::{self, Value};

/// Most rocks one store may list.
const MAX_ROCKS: usize = 5_000;
/// Most module entries one store may list.
const MAX_MODULES: usize = 100_000;

/// What an installed store (`lib/luarocks/rocks-<lua>/manifest`) records:
/// rock name (lowercase) → version → module → file.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct StoreManifest {
    pub rocks: BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StoreError {
    Corrupt(String),
    TooLarge,
}

fn corrupt<T>(reason: impl Into<String>) -> Result<T, StoreError> {
    Err(StoreError::Corrupt(reason.into()))
}

/// Parses a store manifest. The whole store is rejected on any problem: a
/// partial store would turn "not read" into a false "not installed".
pub(crate) fn parse_store_manifest(text: &str) -> Result<StoreManifest, StoreError> {
    let chunk = data::read(text);
    if let Some(stop) = chunk.stop {
        return corrupt(format!("{} (line {})", stop.reason, stop.line));
    }
    let Some(Value::Table(repository)) = chunk.env.get("repository") else {
        return corrupt("no `repository` table");
    };
    let mut out = StoreManifest::default();
    let mut modules = 0usize;
    for (rock, versions) in repository.named() {
        let Value::Table(versions) = versions else {
            return corrupt(format!("`{rock}` is not a table"));
        };
        if out.rocks.len() >= MAX_ROCKS {
            return Err(StoreError::TooLarge);
        }
        let slot = out.rocks.entry(rock.to_lowercase()).or_default();
        for (version, entries) in versions.named() {
            let Value::Table(entries) = entries else {
                return corrupt(format!("`{rock}` {version} is not a table"));
            };
            for entry in entries.positional() {
                let Value::Table(entry) = entry else {
                    return corrupt(format!("`{rock}` {version} has a non-table entry"));
                };
                if entry.get("arch").and_then(Value::as_str) != Some("installed") {
                    continue;
                }
                let files = slot.entry(version.to_string()).or_default();
                match entry.get("modules") {
                    None | Some(Value::Nil) => {}
                    Some(Value::Table(table)) => {
                        for (module, file) in table.named() {
                            let Some(file) = file.as_str() else {
                                return corrupt(format!("module `{module}` has no file"));
                            };
                            modules += 1;
                            if modules > MAX_MODULES {
                                return Err(StoreError::TooLarge);
                            }
                            files.insert(module.to_string(), file.to_string());
                        }
                    }
                    Some(_) => return corrupt(format!("`{rock}` modules is not a table")),
                }
            }
        }
    }
    Ok(out)
}

/// The versions `luarocks.lock` pins: scope (`runtime`, `build`, `test`),
/// rock name, exact version.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Lockfile {
    pub pins: Vec<(&'static str, String, String)>,
}

/// Parses `luarocks.lock`: a Lua module returning
/// `{ dependencies = {...}, build_dependencies = {...}, test_dependencies = {...} }`.
pub(crate) fn parse_lockfile(text: &str) -> Result<Lockfile, String> {
    let chunk = data::read(text);
    if let Some(stop) = chunk.stop {
        return Err(format!("{} (line {})", stop.reason, stop.line));
    }
    let Some(Value::Table(table)) = chunk.returned else {
        return Err("does not return a table".to_string());
    };
    let mut out = Lockfile::default();
    for (key, scope) in [
        ("dependencies", "runtime"),
        ("build_dependencies", "build"),
        ("test_dependencies", "test"),
    ] {
        match table.get(key) {
            None | Some(Value::Nil) => {}
            Some(Value::Table(pins)) => {
                for (name, version) in pins.named() {
                    let Some(version) = version.as_str() else {
                        return Err(format!("`{name}` is pinned to a non-string"));
                    };
                    out.pins
                        .push((scope, name.to_lowercase(), version.to_string()));
                }
            }
            Some(_) => return Err(format!("`{key}` is not a table")),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn reads_the_installed_entries_of_a_real_store_shape() {
        let store = parse_store_manifest(
            "commands = {}\ndependencies = {}\nmodules = { re = { 'lpeg/1.1.0-2' } }\n\
             repository = { lpeg = { ['1.1.0-2'] = { { arch = 'installed', commands = {}, \
             dependencies = {}, modules = { lpeg = 'lpeg.so', re = 're.lua' } } } } }\n",
        )
        .unwrap();
        assert_eq!(store.rocks["lpeg"]["1.1.0-2"]["re"], "re.lua");
    }

    #[test]
    fn non_installed_entries_are_ignored() {
        let store =
            parse_store_manifest("repository = { x = { ['1-1'] = { { arch = 'rockspec' } } } }\n")
                .unwrap();
        assert!(store.rocks["x"].is_empty());
    }

    #[test]
    fn a_store_with_code_or_without_a_repository_is_corrupt() {
        assert!(matches!(
            parse_store_manifest("repository = os.getenv('X')"),
            Err(StoreError::Corrupt(_))
        ));
        assert!(matches!(
            parse_store_manifest("modules = {}"),
            Err(StoreError::Corrupt(_))
        ));
        assert!(matches!(
            parse_store_manifest("repository = { a = { ['1-1'] = { { arch = 'installed', modules = { m = 3 } } } } }"),
            Err(StoreError::Corrupt(_))
        ));
    }

    #[test]
    fn lockfile_pins_carry_their_scope() {
        let lock = parse_lockfile(
            "return {\n dependencies = { lpeg = '1.1.0-2' },\n build_dependencies = {},\n test_dependencies = { busted = '2.1.1-1' },\n}\n",
        )
        .unwrap();
        assert_eq!(
            lock.pins,
            vec![
                ("runtime", "lpeg".to_string(), "1.1.0-2".to_string()),
                ("test", "busted".to_string(), "2.1.1-1".to_string()),
            ]
        );
        assert!(parse_lockfile("return os.getenv('X')").is_err());
        assert!(parse_lockfile("x = 1").is_err());
        assert!(parse_lockfile("return { dependencies = { a = 1 } }").is_err());
    }
}
