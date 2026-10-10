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

mod go;
mod javascript;
mod python;
mod rust;

use go::parse_go_mod;
use javascript::parse_package_json;
use python::parse_requirements_txt;
use rust::parse_cargo_toml;

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
    deps.into_iter()
        .filter(|d| seen.insert(d.name.clone()))
        .collect()
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
        assert!(deps
            .iter()
            .any(|d| d.name == "thiserror" && d.version.as_deref() == Some("2")));
        assert!(deps
            .iter()
            .any(|d| d.name == "serde" && d.version.as_deref() == Some("1.0.229")));
        assert!(
            deps.iter()
                .any(|d| d.name == "mct-core" && d.version.is_none()),
            "path dependency has no version"
        );
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
        assert!(deps
            .iter()
            .any(|d| d.name == "tree-sitter" && d.version.as_deref() == Some("0.25")));
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
        assert!(deps
            .iter()
            .any(|d| d.name == "react" && d.version.as_deref() == Some("^18.3.1")));
        assert!(deps.iter().any(|d| d.name == "typescript"));
        assert!(deps.iter().any(|d| d.name == "react-dom"));
        assert!(deps.iter().any(|d| d.name == "fsevents"));
    }

    #[test]
    fn requirements_txt_skips_comments_options_and_markers() {
        let deps = parse_requirements_txt(
            "# a comment\nrequests==2.31.0\nflask>=2.0,<3.0\nnumpy\nclick[extras]~=8.1.3; python_version >= \"3.8\"\n-e git+https://example.com/foo#egg=foo\n--index-url https://example.com\n",
        );
        assert!(deps
            .iter()
            .any(|d| d.name == "requests" && d.version.as_deref() == Some("==2.31.0")));
        assert!(deps
            .iter()
            .any(|d| d.name == "flask" && d.version.as_deref() == Some(">=2.0,<3.0")));
        assert!(deps
            .iter()
            .any(|d| d.name == "numpy" && d.version.is_none()));
        assert!(
            deps.iter()
                .any(|d| d.name == "click" && d.version.as_deref() == Some("~=8.1.3")),
            "extras and environment marker must be stripped"
        );
        assert_eq!(
            deps.len(),
            4,
            "pip options (-e, --index-url) must not become dependencies: {deps:?}"
        );
    }

    #[test]
    fn go_mod_reads_block_and_single_line_require() {
        let deps = parse_go_mod(
            "module example.com/app\n\ngo 1.21\n\nrequire (\n\tgithub.com/foo/bar v1.2.3\n\tgithub.com/baz/qux v0.5.0 // indirect\n)\n\nrequire github.com/single/dep v2.0.0\n",
        );
        assert!(deps
            .iter()
            .any(|d| d.name == "github.com/foo/bar" && d.version.as_deref() == Some("v1.2.3")));
        assert!(
            deps.iter()
                .any(|d| d.name == "github.com/baz/qux" && d.version.as_deref() == Some("v0.5.0")),
            "trailing // indirect comment must be stripped"
        );
        assert!(deps
            .iter()
            .any(|d| d.name == "github.com/single/dep" && d.version.as_deref() == Some("v2.0.0")));
        assert_eq!(deps.len(), 3);
    }

    #[test]
    fn a_name_in_several_sections_is_kept_once_with_its_first_version() {
        let cargo = parse_manifest(
            "Cargo.toml",
            "[dependencies]\ntokio = { version = \"1.53.1\", features = [\"rt\"] }\n\n[dev-dependencies]\ntokio = { version = \"1.53.1\", features = [\"io-util\"] }\nproptest = \"1\"\n\n[build-dependencies]\ntokio = \"1\"\n",
        );
        assert_eq!(
            cargo.iter().filter(|d| d.name == "tokio").count(),
            1,
            "{cargo:?}"
        );
        assert!(cargo
            .iter()
            .any(|d| d.name == "tokio" && d.version.as_deref() == Some("1.53.1")));
        assert_eq!(cargo.len(), 2);

        let npm = parse_manifest(
            "package.json",
            r#"{"dependencies": {"react": "^18.3.1"}, "devDependencies": {"react": "18.0.0"}}"#,
        );
        assert_eq!(
            npm,
            vec![ManifestDependency {
                name: "react".into(),
                version: Some("^18.3.1".into())
            }]
        );

        assert_eq!(
            parse_manifest("requirements.txt", "flask==2.3.0\nflask==2.3.0\n").len(),
            1
        );
        assert_eq!(
            parse_manifest(
                "go.mod",
                "module m\n\nrequire a.com/b v1.0.0\nrequire a.com/b v1.0.0\n"
            )
            .len(),
            1
        );
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
