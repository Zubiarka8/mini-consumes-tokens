//! Javascript dependency declarations.

use super::ManifestDependency;

/// `dependencies`/`devDependencies`/`peerDependencies`/`optionalDependencies`
/// — npm/pnpm/yarn all read the same four keys.
pub(super) fn parse_package_json(contents: &str) -> Vec<ManifestDependency> {
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(contents) else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(obj) = doc.get(section).and_then(serde_json::Value::as_object) {
            for (name, value) in obj {
                let version = value.as_str().map(str::to_string);
                deps.push(ManifestDependency {
                    name: name.clone(),
                    version,
                });
            }
        }
    }
    deps
}
