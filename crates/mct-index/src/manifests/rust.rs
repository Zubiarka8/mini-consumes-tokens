//! Rust dependency declarations.

use super::ManifestDependency;

pub(crate) fn cargo_library_name(contents: &str) -> Option<String> {
    let doc = contents.parse::<toml::Table>().ok()?;
    let package = doc.get("package")?.as_table()?;
    let name = doc
        .get("lib")
        .and_then(toml::Value::as_table)
        .and_then(|lib| lib.get("name"))
        .and_then(toml::Value::as_str)
        .or_else(|| package.get("name").and_then(toml::Value::as_str))?;
    Some(name.replace('-', "_"))
}

/// `(alias, path)` for each renamed path dependency
/// (`alias = { package = "real", path = "../real" }`) in `[dependencies]`,
/// `[dev-dependencies]` and `[build-dependencies]`: the dependent's code
/// writes `use alias::…` for the crate at `path` (issue #563). The alias
/// has dashes read as underscores, as rustc does.
pub(crate) fn cargo_path_renames(contents: &str) -> Vec<(String, String)> {
    let Ok(doc) = contents.parse::<toml::Table>() else {
        return Vec::new();
    };
    let mut renames = Vec::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(table) = doc.get(section).and_then(toml::Value::as_table) else {
            continue;
        };
        for (alias, spec) in table {
            let Some(spec) = spec.as_table() else {
                continue;
            };
            let (Some(_), Some(path)) = (
                spec.get("package").and_then(toml::Value::as_str),
                spec.get("path").and_then(toml::Value::as_str),
            ) else {
                continue;
            };
            let pair = (alias.replace('-', "_"), path.to_string());
            if !renames.contains(&pair) {
                renames.push(pair);
            }
        }
    }
    renames
}

/// `[dependencies]`/`[dev-dependencies]`/`[build-dependencies]` at the
/// document root, plus `[workspace.dependencies]` for a workspace root
/// manifest — not `[target.'cfg(...)'.dependencies]`, a deliberately
/// uncommon case left out.
pub(super) fn parse_cargo_toml(contents: &str) -> Vec<ManifestDependency> {
    let Ok(doc) = contents.parse::<toml::Table>() else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = doc.get(section).and_then(toml::Value::as_table) {
            collect_cargo_table(table, &mut deps);
        }
    }
    if let Some(table) = doc
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|w| w.get("dependencies"))
        .and_then(toml::Value::as_table)
    {
        collect_cargo_table(table, &mut deps);
    }
    deps
}

fn collect_cargo_table(
    table: &toml::map::Map<String, toml::Value>,
    out: &mut Vec<ManifestDependency>,
) {
    for (name, value) in table {
        let version = match value {
            toml::Value::String(v) => Some(v.clone()),
            toml::Value::Table(t) => t
                .get("version")
                .and_then(toml::Value::as_str)
                .map(str::to_string),
            _ => None,
        };
        out.push(ManifestDependency {
            name: name.clone(),
            version,
        });
    }
}

#[cfg(test)]
mod rename_tests {
    use super::cargo_path_renames;

    #[test]
    fn only_renamed_path_dependencies_are_kept() {
        let toml = "[dependencies]\n\
            my-store = { package = 'grep-store', path = '../store' }\n\
            plain = { path = '../plain' }\n\
            registry = { package = 'real', version = '1' }\n\
            text = '1'\n\
            [dev-dependencies]\n\
            fixture = { package = 'fx', path = 'fixtures/fx' }\n";
        assert_eq!(
            cargo_path_renames(toml),
            [
                ("my_store".to_string(), "../store".to_string()),
                ("fixture".to_string(), "fixtures/fx".to_string()),
            ]
        );
        assert!(cargo_path_renames("not toml [").is_empty());
    }
}
