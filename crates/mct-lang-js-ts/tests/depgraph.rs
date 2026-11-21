//! File-level `Imports` evidence: each cross-file import names the target
//! file's `Module` symbol and carries enough evidence to resolve to it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::SymbolKind;
use mct_index::Index;
use mct_lang_js_ts::JsTsParser;

fn parse(path: &str, src: &str) -> mct_core::ParsedFile {
    mct_corpus::parse_source(&JsTsParser, path, "JS/TS", src)
}

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser))
}

use mct_corpus::import_target;

#[test]
fn tsconfig_path_alias_resolves_to_the_target_file() {
    let index = open_indexed();
    assert_eq!(
        import_target(&index, "utility", "alias.ts").as_deref(),
        Some("src/utility.ts")
    );
}

#[test]
fn tsconfig_changes_and_multiple_existing_targets_are_reflected() {
    let root = std::env::temp_dir().join(format!("mct-ts-alias-{}", std::process::id()));
    std::fs::create_dir_all(root.join("first")).unwrap();
    std::fs::create_dir_all(root.join("second")).unwrap();
    std::fs::write(root.join("app.ts"), "import { value } from '@/value';\n").unwrap();
    std::fs::write(root.join("first/value.ts"), "export function value() {}\n").unwrap();
    std::fs::write(root.join("second/value.ts"), "export function value() {}\n").unwrap();
    let config = root.join("tsconfig.json");
    std::fs::write(&config, "{\n// compiler settings\n\"compilerOptions\": {\"paths\": {\"@/*\": [\"first/*\", \"second/*\",],},},\n}").unwrap();
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(JsTsParser));
    let mut index = Index::open_in_memory(&root, mct_index::ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let import = |index: &Index| {
        index
            .find_references("value")
            .unwrap()
            .into_iter()
            .find(|hit| {
                hit.kind == "imports" && hit.relative_path == "app.ts" && hit.candidates.len() == 2
            })
    };
    assert!(
        import(&index).is_some(),
        "both existing targets must stay candidates"
    );
    std::fs::write(
        &config,
        r#"{"compilerOptions":{"paths":{"@/*":["first/*"]}}}"#,
    )
    .unwrap();
    index
        .reindex_paths(&registry, &["tsconfig.json".into()])
        .unwrap();
    assert_eq!(
        import_target(&index, "value", "app.ts").as_deref(),
        Some("first/value.ts")
    );
    std::fs::remove_file(&config).unwrap();
    index
        .reindex_paths(&registry, &["tsconfig.json".into()])
        .unwrap();
    assert_eq!(import_target(&index, "value", "app.ts"), None);
    std::fs::remove_dir_all(root).unwrap();
}

/// Indexes `files` (path, contents) in a fresh temporary directory.
fn index_files(
    name: &str,
    files: &[(&str, &str)],
) -> (std::path::PathBuf, Index, mct_core::LanguageRegistry) {
    let root = std::env::temp_dir().join(format!("mct-ts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, contents) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(JsTsParser));
    let mut index = Index::open_in_memory(&root, mct_index::ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    (root, index, registry)
}

#[test]
fn an_extended_config_supplies_paths_relative_to_itself() {
    let (root, mut index, registry) = index_files(
        "extends",
        &[
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"paths":{"@/*":["src/*"]}}}"#,
            ),
            (
                "web/tsconfig.json",
                r#"{"extends":"../tsconfig","compilerOptions":{"strict":true}}"#,
            ),
            ("web/app.ts", "import { value } from '@/value';\n"),
            ("src/value.ts", "export function value() {}\n"),
            ("lib/value.ts", "export function value() {}\n"),
        ],
    );
    assert_eq!(
        import_target(&index, "value", "web/app.ts").as_deref(),
        Some("src/value.ts"),
        "the inherited `paths` resolve from the base config's directory"
    );
    // Only the base changes: the extending config must follow it.
    std::fs::write(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"paths":{"@/*":["lib/*"]}}}"#,
    )
    .unwrap();
    index
        .reindex_paths(&registry, &["tsconfig.json".into()])
        .unwrap();
    assert_eq!(
        import_target(&index, "value", "web/app.ts").as_deref(),
        Some("lib/value.ts")
    );
    std::fs::remove_file(root.join("tsconfig.json")).unwrap();
    index
        .reindex_paths(&registry, &["tsconfig.json".into()])
        .unwrap();
    assert_eq!(import_target(&index, "value", "web/app.ts"), None);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn own_paths_override_inherited_ones_and_cycles_or_packages_give_none() {
    let (root, index, _registry) = index_files(
        "override",
        &[
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"paths":{"@/*":["src/*"]}}}"#,
            ),
            (
                "web/tsconfig.json",
                r#"{"extends":["@tsconfig/node18/tsconfig.json","../tsconfig.json"],"compilerOptions":{"paths":{"@/*":["own/*"]}}}"#,
            ),
            ("web/app.ts", "import { value } from '@/value';\n"),
            ("web/own/value.ts", "export function value() {}\n"),
            ("src/value.ts", "export function value() {}\n"),
            ("a/tsconfig.json", r#"{"extends":"../b/tsconfig.json"}"#),
            ("b/tsconfig.json", r#"{"extends":"../a/tsconfig.json"}"#),
            ("a/main.ts", "import { value } from '@/value';\n"),
        ],
    );
    assert_eq!(
        import_target(&index, "value", "web/app.ts").as_deref(),
        Some("web/own/value.ts")
    );
    assert_eq!(
        import_target(&index, "value", "a/main.ts"),
        None,
        "an `extends` cycle declares no paths"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn indexing_status_lists_each_alias_source_and_an_unreadable_one_resolves_nothing() {
    let (root, index, _registry) = index_files(
        "status",
        &[
            (
                "tsconfig.json",
                r#"{"compilerOptions":{"paths":{"@/*":["src/*"]}}}"#,
            ),
            ("web/tsconfig.json", r#"{"compilerOptions": {"paths": "#),
            ("web/app.ts", "import { value } from '@/value';\n"),
            ("main.ts", "import { value } from '@/value';\n"),
            ("src/value.ts", "export function value() {}\n"),
        ],
    );
    let sources: Vec<(String, usize, bool)> = index
        .status()
        .unwrap()
        .alias_sources
        .into_iter()
        .map(|source| (source.manifest_path, source.aliases, source.error.is_some()))
        .collect();
    assert_eq!(
        sources,
        [
            ("tsconfig.json".to_string(), 1, false),
            ("web/tsconfig.json".to_string(), 0, true)
        ]
    );
    assert_eq!(
        import_target(&index, "value", "main.ts").as_deref(),
        Some("src/value.ts")
    );
    assert_eq!(
        import_target(&index, "value", "web/app.ts"),
        None,
        "the outer config must not resolve under an unreadable nearer one"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn relative_imports_resolve_to_the_target_files_module() {
    let index = open_indexed();
    assert_eq!(
        import_target(&index, "util", "app.ts").as_deref(),
        Some("lib/util.ts")
    );
    assert_eq!(
        import_target(&index, "legacy", "app.ts").as_deref(),
        Some("legacy.ts")
    );
    assert_eq!(
        import_target(&index, "old", "app.ts").as_deref(),
        Some("lib/old.js")
    );
    assert_eq!(
        import_target(&index, "lazy", "app.ts").as_deref(),
        Some("lib/lazy.ts")
    );
    assert_eq!(
        import_target(&index, "util", "reexport.ts").as_deref(),
        Some("lib/util.ts")
    );
}

#[test]
fn evidence_is_set_only_where_the_source_proves_it() {
    let parsed = parse(
        "src/app.ts",
        "import a from \"./a\";\nimport b from \"./b.ts\";\nimport c from \"react\";\nimport d from \"node:fs\";\nimport e from \"./data.json\";\n",
    );
    let evidence = |name: &str| mct_corpus::import_evidence(&parsed, name);
    // Extensionless: only the stem and the directory are proven.
    let a = evidence("a").unwrap();
    assert_eq!(
        (a.module.as_deref(), a.path.as_deref()),
        (Some("src"), None)
    );
    assert_eq!(a.kind, Some(SymbolKind::Module));
    // Literal TypeScript extension: the exact file is named.
    assert_eq!(evidence("b").unwrap().path.as_deref(), Some("src/b.ts"));
    // A `node:` builtin is provably outside the repository.
    assert!(evidence("node:fs").unwrap().external);
    // A bare specifier or an asset is neither external nor a local module.
    assert!(!parsed.relations.iter().any(|r| r.to_name == "react"));
    assert!(!parsed.relations.iter().any(|r| r.to_name == "data"));
}

/// Issue #563 (Node): a `#…` specifier resolves through the nearest
/// `package.json` `imports` map. A wildcard key, an exact key and a
/// condition object all count. A `.js` target also finds its `.ts` source.
/// A nested package.json without `imports` governs its own files, so the
/// outer map does not leak into it.
#[test]
fn package_json_imports_resolve_through_the_nearest_package() {
    let (root, mut index, registry) = index_files(
        "pkg-imports",
        &[
            (
                "package.json",
                r##"{"name":"app","imports":{"#lib/*":"./src/lib/*.js","#db":{"node":"./src/database.ts","default":"./src/missing.ts"}}}"##,
            ),
            (
                "src/main.ts",
                "import { util } from '#lib/util';\nimport { db } from '#db';\n",
            ),
            ("src/lib/util.ts", "export function util() {}\n"),
            ("src/database.ts", "export const db = 1;\n"),
            ("nested/package.json", r#"{"name":"nested"}"#),
            ("nested/use.ts", "import { util } from '#lib/util';\n"),
        ],
    );
    assert_eq!(
        import_target(&index, "util", "src/main.ts").as_deref(),
        Some("src/lib/util.ts")
    );
    assert_eq!(
        import_target(&index, "db", "src/main.ts").as_deref(),
        Some("src/database.ts")
    );
    assert_eq!(import_target(&index, "util", "nested/use.ts"), None);

    std::fs::write(root.join("package.json"), r#"{"name":"app"}"#).unwrap();
    index
        .reindex_paths(&registry, &["package.json".into()])
        .unwrap();
    assert_eq!(import_target(&index, "util", "src/main.ts"), None);
    std::fs::remove_dir_all(root).unwrap();
}

/// Issue #563 (npm workspaces): a named import from a bare specifier that
/// names a workspace package (`"name": "@acme/ui"` in a folder called
/// `design-system`) resolves to that package's files only, even against a
/// file whose path holds the module name. A bare import of
/// a package the repository does not hold (`react`) gains nothing, and
/// renaming the package drops it from the candidates.
#[test]
fn a_workspace_package_import_resolves_inside_that_package() {
    let (root, mut index, registry) = index_files(
        "workspace",
        &[
            ("packages/design-system/package.json", r#"{"name":"@acme/ui"}"#),
            ("packages/design-system/src/index.ts", "export function Button() {}\n"),
            ("packages/other/src/widgets.ts", "export function Button() {}\n"),
            // Its path holds `/ui/`, the module name: still not the package.
            ("apps/web/src/ui/button.ts", "export function Button() {}\n"),
            (
                "apps/web/src/main.ts",
                "import { Button } from '@acme/ui';\nimport { useState } from 'react';\nButton();\n",
            ),
        ],
    );
    let binding = |index: &Index, name: &str| {
        index
            .find_references(name)
            .unwrap()
            .into_iter()
            .find(|hit| hit.kind == "imports" && hit.relative_path == "apps/web/src/main.ts")
            .unwrap()
    };
    let hit = binding(&index, "Button");
    assert_eq!(hit.resolution, mct_index::Resolution::Resolved, "{hit:?}");
    assert_eq!(
        hit.candidates[0].relative_path,
        "packages/design-system/src/index.ts"
    );
    assert_ne!(
        binding(&index, "useState").resolution,
        mct_index::Resolution::Resolved
    );

    std::fs::write(
        root.join("packages/design-system/package.json"),
        r#"{"name":"@acme/design"}"#,
    )
    .unwrap();
    index
        .reindex_paths(&registry, &["packages/design-system/package.json".into()])
        .unwrap();
    // Back to the module-name rule, which knows no package.
    let after = binding(&index, "Button");
    assert!(
        after
            .candidates
            .iter()
            .all(|c| !c.relative_path.starts_with("packages/design-system/")),
        "{after:?}"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// A CommonJS `const { Button } = require('@acme/ui')` binds `Button` like
/// the ES import does: it resolves inside the workspace package only, and
/// a renamed (`{ Button: Btn }`) binding is named by its local name.
#[test]
fn a_destructured_require_of_a_workspace_package_resolves_inside_that_package() {
    let (root, index, _registry) = index_files(
        "workspace-require",
        &[
            (
                "packages/design-system/package.json",
                r#"{"name":"@acme/ui"}"#,
            ),
            (
                "packages/design-system/src/index.js",
                "function Button() {}\nfunction Card() {}\nmodule.exports = { Button, Card };\n",
            ),
            (
                "packages/other/src/widgets.js",
                "function Button() {}\nfunction Card() {}\n",
            ),
            (
                "apps/web/src/main.js",
                "const { Button, Card: Tile } = require('@acme/ui');\nButton();\nTile();\n",
            ),
        ],
    );
    let binding = |name: &str| {
        index
            .find_references(name)
            .unwrap()
            .into_iter()
            .find(|hit| hit.kind == "imports" && hit.relative_path == "apps/web/src/main.js")
            .unwrap_or_else(|| panic!("no import binding for {name}"))
    };
    for name in ["Button", "Tile"] {
        let hit = binding(name);
        assert_eq!(hit.relative_path, "apps/web/src/main.js");
        assert!(
            hit.candidates
                .iter()
                .all(|c| c.relative_path == "packages/design-system/src/index.js"),
            "{name}: {hit:?}"
        );
    }
    assert_eq!(
        binding("Button").resolution,
        mct_index::Resolution::Resolved
    );
    std::fs::remove_dir_all(root).unwrap();
}
