//! File-level `Imports` evidence: each cross-file import names the target
//! file's `Module` symbol and carries enough evidence to resolve to it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::{LanguageParser, LanguageRegistry, RelationKind, SourceFile, SymbolKind};
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_python::PythonParser;

fn parse(path: &str, src: &str) -> mct_core::ParsedFile {
    PythonParser
        .parse(&SourceFile {
            relative_path: path.to_string(),
            contents: src.to_string(),
        })
        .unwrap()
}

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(PythonParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    index
}

/// The file the `Module`-targeting `imports` relation named `to` from
/// `from_file` resolves to.
fn import_target(index: &Index, to: &str, from_file: &str) -> Option<String> {
    let hit = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "imports" && h.relative_path == from_file && h.to_name == to)?;
    (hit.resolution == Resolution::Resolved).then(|| hit.candidates[0].relative_path.clone())
}

#[test]
fn module_imports_resolve_to_the_imported_file() {
    let index = open_indexed();
    assert_eq!(
        import_target(&index, "helpers", "main.py").as_deref(),
        Some("pkg/helpers.py")
    );
    assert_eq!(
        import_target(&index, "models", "main.py").as_deref(),
        Some("pkg/models.py")
    );
    // `import pkg.sub.deep as deep`: the real module, not just the alias.
    assert_eq!(
        import_target(&index, "deep", "main.py").as_deref(),
        Some("pkg/sub/deep.py")
    );
    // Relative imports resolve through the importing file's package.
    assert_eq!(
        import_target(&index, "helpers", "pkg/models.py").as_deref(),
        Some("pkg/helpers.py")
    );
    assert_eq!(
        import_target(&index, "helpers", "pkg/sub/deep.py").as_deref(),
        Some("pkg/helpers.py")
    );
}

#[test]
fn evidence_is_set_only_where_the_source_proves_it() {
    let parsed = parse(
        "app/main.py",
        "import os\nimport a.b.c\nfrom .x import y\nfrom . import z\n",
    );
    let evidence = |name: &str| {
        let (index, _) = parsed
            .relations
            .iter()
            .enumerate()
            .find(|(_, r)| r.kind == RelationKind::Imports && r.to_name == name)
            .unwrap_or_else(|| panic!("no import {name}"));
        parsed
            .relation_targets
            .iter()
            .find(|t| t.relation == index)
            .cloned()
    };
    let c = evidence("c").unwrap();
    assert_eq!(
        (c.module.as_deref(), c.kind),
        (Some("b"), Some(SymbolKind::Module))
    );
    // `os` could be the stdlib or a local file: no `module`, never external.
    let os = evidence("os").unwrap();
    assert_eq!((os.module, os.external), (None, false));
    // `from .x import y` stays within this file's package directory.
    assert_eq!(evidence("x").unwrap().module.as_deref(), Some("app"));
    // `from . import z` names no module file of its own.
    assert!(evidence("z").is_none());
}
