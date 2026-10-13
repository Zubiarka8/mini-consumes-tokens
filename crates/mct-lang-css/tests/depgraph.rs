//! File-level `Imports` evidence: an `@import` of a local `.css` file names
//! that file's `Module` symbol, pinned to its path.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::{LanguageParser, LanguageRegistry, RelationKind, SourceFile, SymbolKind};
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_css::CssParser;

#[test]
fn an_imported_stylesheet_resolves_to_its_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/theme");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(CssParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let hit = index
        .find_references("extra")
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "imports" && h.relative_path == "main.css")
        .unwrap();
    assert_eq!(hit.resolution, Resolution::Resolved);
    assert_eq!(hit.candidates[0].relative_path, "extra.css");
}

#[test]
fn evidence_is_set_only_where_the_source_proves_it() {
    let parsed = CssParser
        .parse(&SourceFile {
            relative_path: "styles/app.css".to_string(),
            contents: concat!(
                "@import \"../base/reset.css\";\n",
                "@import url(\"https://cdn.example.com/lib.css\");\n",
                "@import \"tailwindcss\";\n"
            )
            .to_string(),
        })
        .unwrap();
    let evidence = |name: &str| {
        let i = parsed
            .relations
            .iter()
            .position(|r| r.kind == RelationKind::Imports && r.to_name == name)?;
        parsed
            .relation_targets
            .iter()
            .find(|t| t.relation == i)
            .cloned()
    };
    let reset = evidence("reset").unwrap();
    assert_eq!(reset.path.as_deref(), Some("base/reset.css"));
    assert_eq!(reset.kind, Some(SymbolKind::Module));
    assert!(
        evidence("https://cdn.example.com/lib.css")
            .unwrap()
            .external
    );
    // A bare package name is neither external nor a file path.
    assert!(evidence("tailwindcss").is_none());
}
