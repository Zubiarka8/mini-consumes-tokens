//! Dependency-manifest detection: a best-effort parse of a handful of
//! well-known manifest file *names* (not extensions, and not routed through
//! `LanguageParser` — a manifest isn't source code to symbol-index) found
//! while walking a repo, recording what each declares in the `dependencies`
//! table.
//!
//! Deliberately narrow scope: one manifest format per language this
//! workspace already has a `LanguageParser` for, and only the ones with a
//! single, stable, machine-readable format (Java's Maven vs. Gradle split,
//! and Gradle's own Groovy/Kotlin DSL in particular, is not one — left out).
//! A version can legitimately be absent (path/git dependencies, or a
//! `{ workspace = true }` entry whose real version lives in the workspace
//! root's own manifest, itself scanned as its own file) — recorded as
//! `None`, not skipped.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestDependency {
    pub name: String,
    pub version: Option<String>,
}

/// The ecosystem a recognized manifest file name belongs to, or `None` if
/// `file_name` isn't one this module knows how to parse.
pub fn manifest_language(file_name: &str) -> Option<&'static str> {
    match file_name {
        "Cargo.toml" => Some("rust"),
        "package.json" => Some("javascript_typescript"),
        "requirements.txt" => Some("python"),
        "go.mod" => Some("go"),
        _ => None,
    }
}

/// One entry per dependency name: a name declared in several sections
/// (`[dependencies]` and `[dev-dependencies]`, `dependencies` and
/// `devDependencies`, a repeated `requirements.txt` line or `go.mod`
/// `require`) keeps its first occurrence, which is the primary section since
/// each parser reads those first. The `dependencies` table is
/// `UNIQUE(manifest_path, name)`, so a duplicate would otherwise fail the
/// whole reindex.
pub fn parse_manifest(file_name: &str, contents: &str) -> Vec<ManifestDependency> {
    let deps = match file_name {
        "Cargo.toml" => parse_cargo_toml(contents),
        "package.json" => parse_package_json(contents),
        "requirements.txt" => parse_requirements_txt(contents),
        "go.mod" => parse_go_mod(contents),
        _ => Vec::new(),
    };
    let mut seen = std::collections::HashSet::new();
    deps.into_iter().filter(|d| seen.insert(d.name.clone())).collect()
}

/// `[dependencies]`/`[dev-dependencies]`/`[build-dependencies]` at the
/// document root, plus `[workspace.dependencies]` for a workspace root
/// manifest — not `[target.'cfg(...)'.dependencies]`, a deliberately
/// uncommon case left out.
fn parse_cargo_toml(contents: &str) -> Vec<ManifestDependency> {
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

fn collect_cargo_table(table: &toml::map::Map<String, toml::Value>, out: &mut Vec<ManifestDependency>) {
    for (name, value) in table {
        let version = match value {
            toml::Value::String(v) => Some(v.clone()),
            toml::Value::Table(t) => t.get("version").and_then(toml::Value::as_str).map(str::to_string),
            _ => None,
        };
        out.push(ManifestDependency { name: name.clone(), version });
    }
}

/// `dependencies`/`devDependencies`/`peerDependencies`/`optionalDependencies`
/// — npm/pnpm/yarn all read the same four keys.
fn parse_package_json(contents: &str) -> Vec<ManifestDependency> {
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(contents) else {
        return Vec::new();
    };
    let mut deps = Vec::new();
    for section in ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"] {
        if let Some(obj) = doc.get(section).and_then(serde_json::Value::as_object) {
            for (name, value) in obj {
                let version = value.as_str().map(str::to_string);
                deps.push(ManifestDependency { name: name.clone(), version });
            }
        }
    }
    deps
}

/// One requirement per non-comment, non-option line. The version field
/// keeps the constraint as written (`">=2.0,<3.0"`), not a single resolved
/// version — pip itself doesn't resolve to one without a lockfile either.
fn parse_requirements_txt(contents: &str) -> Vec<ManifestDependency> {
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
        deps.push(ManifestDependency { name: name.to_string(), version });
    }
    deps
}

fn split_requirement(spec: &str) -> (&str, Option<String>) {
    match spec.find(['=', '>', '<', '!', '~']) {
        Some(idx) if idx > 0 => (spec[..idx].trim(), Some(spec[idx..].trim().to_string())),
        _ => (spec.trim(), None),
    }
}

/// Both `require (\n module version\n)` block form and single-line
/// `require module version` — `go.sum` (resolved, transitive-included
/// checksums) is deliberately not read, `go.mod` alone is the direct
/// dependency declaration, same "one manifest, direct deps" scope as the
/// other three formats here.
fn parse_go_mod(contents: &str) -> Vec<ManifestDependency> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_toml_reads_plain_and_inline_table_versions() {
        let deps = parse_cargo_toml(
            r#"
            [package]
            name = "demo"

            [dependencies]
            thiserror = "2"
            serde = { version = "1.0.229", features = ["derive"] }
            mct-core = { path = "../mct-core" }

            [dev-dependencies]
            proptest = "1"
            "#,
        );
        assert!(deps.iter().any(|d| d.name == "thiserror" && d.version.as_deref() == Some("2")));
        assert!(deps.iter().any(|d| d.name == "serde" && d.version.as_deref() == Some("1.0.229")));
        assert!(deps.iter().any(|d| d.name == "mct-core" && d.version.is_none()), "path dependency has no version");
        assert!(deps.iter().any(|d| d.name == "proptest"));
    }

    #[test]
    fn cargo_toml_reads_workspace_dependencies_table() {
        let deps = parse_cargo_toml(
            r#"
            [workspace]
            members = ["crates/a"]

            [workspace.dependencies]
            tree-sitter = "0.25"
            "#,
        );
        assert!(deps.iter().any(|d| d.name == "tree-sitter" && d.version.as_deref() == Some("0.25")));
    }

    #[test]
    fn package_json_reads_all_four_dependency_sections() {
        let deps = parse_package_json(
            r#"{
                "dependencies": { "react": "^18.3.1" },
                "devDependencies": { "typescript": "5.6.0" },
                "peerDependencies": { "react-dom": "^18.0.0" },
                "optionalDependencies": { "fsevents": "2.3.0" }
            }"#,
        );
        assert!(deps.iter().any(|d| d.name == "react" && d.version.as_deref() == Some("^18.3.1")));
        assert!(deps.iter().any(|d| d.name == "typescript"));
        assert!(deps.iter().any(|d| d.name == "react-dom"));
        assert!(deps.iter().any(|d| d.name == "fsevents"));
    }

    #[test]
    fn requirements_txt_skips_comments_options_and_markers() {
        let deps = parse_requirements_txt(
            "# a comment\nrequests==2.31.0\nflask>=2.0,<3.0\nnumpy\nclick[extras]~=8.1.3; python_version >= \"3.8\"\n-e git+https://example.com/foo#egg=foo\n--index-url https://example.com\n",
        );
        assert!(deps.iter().any(|d| d.name == "requests" && d.version.as_deref() == Some("==2.31.0")));
        assert!(deps.iter().any(|d| d.name == "flask" && d.version.as_deref() == Some(">=2.0,<3.0")));
        assert!(deps.iter().any(|d| d.name == "numpy" && d.version.is_none()));
        assert!(deps.iter().any(|d| d.name == "click" && d.version.as_deref() == Some("~=8.1.3")), "extras and environment marker must be stripped");
        assert_eq!(deps.len(), 4, "pip options (-e, --index-url) must not become dependencies: {deps:?}");
    }

    #[test]
    fn go_mod_reads_block_and_single_line_require() {
        let deps = parse_go_mod(
            "module example.com/app\n\ngo 1.21\n\nrequire (\n\tgithub.com/foo/bar v1.2.3\n\tgithub.com/baz/qux v0.5.0 // indirect\n)\n\nrequire github.com/single/dep v2.0.0\n",
        );
        assert!(deps.iter().any(|d| d.name == "github.com/foo/bar" && d.version.as_deref() == Some("v1.2.3")));
        assert!(deps.iter().any(|d| d.name == "github.com/baz/qux" && d.version.as_deref() == Some("v0.5.0")), "trailing // indirect comment must be stripped");
        assert!(deps.iter().any(|d| d.name == "github.com/single/dep" && d.version.as_deref() == Some("v2.0.0")));
        assert_eq!(deps.len(), 3);
    }

    #[test]
    fn a_name_in_several_sections_is_kept_once_with_its_first_version() {
        let cargo = parse_manifest(
            "Cargo.toml",
            "[dependencies]\ntokio = { version = \"1.53.1\", features = [\"rt\"] }\n\n[dev-dependencies]\ntokio = { version = \"1.53.1\", features = [\"io-util\"] }\nproptest = \"1\"\n\n[build-dependencies]\ntokio = \"1\"\n",
        );
        assert_eq!(cargo.iter().filter(|d| d.name == "tokio").count(), 1, "{cargo:?}");
        assert!(cargo.iter().any(|d| d.name == "tokio" && d.version.as_deref() == Some("1.53.1")));
        assert_eq!(cargo.len(), 2);

        let npm = parse_manifest("package.json", r#"{"dependencies": {"react": "^18.3.1"}, "devDependencies": {"react": "18.0.0"}}"#);
        assert_eq!(npm, vec![ManifestDependency { name: "react".into(), version: Some("^18.3.1".into()) }]);

        assert_eq!(parse_manifest("requirements.txt", "flask==2.3.0\nflask==2.3.0\n").len(), 1);
        assert_eq!(parse_manifest("go.mod", "module m\n\nrequire a.com/b v1.0.0\nrequire a.com/b v1.0.0\n").len(), 1);
    }

    #[test]
    fn unrecognized_file_name_is_not_parsed() {
        assert_eq!(manifest_language("setup.py"), None);
        assert!(parse_manifest("setup.py", "anything").is_empty());
    }

    #[test]
    fn malformed_manifest_returns_empty_not_panic() {
        assert!(parse_cargo_toml("not [ valid toml").is_empty());
        assert!(parse_package_json("not json").is_empty());
    }
}
