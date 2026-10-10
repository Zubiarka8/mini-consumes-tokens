//! Rust dependency declarations.

use super::ManifestDependency;

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
