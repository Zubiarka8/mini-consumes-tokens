//! PHP (Composer) dependency declarations.

use super::ManifestDependency;

/// `require` and `require-dev` of a `composer.json`. Only entries that name a
/// package (`vendor/package`) are dependencies: `php`, `ext-*`, `lib-*` and
/// `composer-plugin-api` are platform requirements Composer checks against
/// the running PHP, not code that gets installed.
pub(super) fn parse_composer_json(contents: &str) -> Vec<ManifestDependency> {
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(contents) else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    for section in ["require", "require-dev"] {
        let Some(entries) = doc.get(section).and_then(serde_json::Value::as_object) else {
            continue;
        };
        for (name, value) in entries {
            if !name.contains('/') {
                continue;
            }
            deps.push(ManifestDependency {
                name: name.clone(),
                version: value.as_str().map(str::to_string),
            });
        }
    }
    deps
}
