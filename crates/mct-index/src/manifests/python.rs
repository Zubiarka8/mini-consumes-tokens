//! Python dependency declarations.

use super::ManifestDependency;

/// One requirement per non-comment, non-option line. The version field
/// keeps the constraint as written (`">=2.0,<3.0"`), not a single resolved
/// version — pip itself doesn't resolve to one without a lockfile either.
pub(super) fn parse_requirements_txt(contents: &str) -> Vec<ManifestDependency> {
    let mut deps = Vec::new();
    for raw_line in contents.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('-') {
            continue; // blank, comment-only, or a pip option (-r, -e, --index-url, ...)
        }
        let line = line.split(';').next().unwrap_or(line).trim(); // drop environment markers
        let (name_part, version) = split_requirement(line);
        let name = name_part.split('[').next().unwrap_or(name_part).trim(); // drop [extras]
        if name.is_empty() {
            continue;
        }
        deps.push(ManifestDependency {
            name: name.to_string(),
            version,
        });
    }
    deps
}

fn split_requirement(spec: &str) -> (&str, Option<String>) {
    match spec.find(['=', '>', '<', '!', '~']) {
        Some(idx) if idx > 0 => (spec[..idx].trim(), Some(spec[idx..].trim().to_string())),
        _ => (spec.trim(), None),
    }
}
