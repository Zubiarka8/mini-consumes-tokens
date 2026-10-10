//! Go dependency declarations.

use super::ManifestDependency;

/// Both `require (\n module version\n)` block form and single-line
/// `require module version` — `go.sum` (resolved, transitive-included
/// checksums) is deliberately not read, `go.mod` alone is the direct
/// dependency declaration, same "one manifest, direct deps" scope as the
/// other three formats here.
pub(super) fn parse_go_mod(contents: &str) -> Vec<ManifestDependency> {
    let mut deps = Vec::new();
    let mut in_require_block = false;
    for raw_line in contents.lines() {
        let line = raw_line.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if in_require_block {
            if line == ")" {
                in_require_block = false;
            } else if let Some(dep) = parse_go_require_entry(line) {
                deps.push(dep);
            }
            continue;
        }
        if line == "require (" {
            in_require_block = true;
        } else if let Some(rest) = line.strip_prefix("require ") {
            if let Some(dep) = parse_go_require_entry(rest.trim()) {
                deps.push(dep);
            }
        }
    }
    deps
}

fn parse_go_require_entry(entry: &str) -> Option<ManifestDependency> {
    let mut parts = entry.split_whitespace();
    let name = parts.next()?.to_string();
    let version = parts.next().map(str::to_string);
    Some(ManifestDependency { name, version })
}
