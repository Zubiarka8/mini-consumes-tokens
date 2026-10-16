//! File-level `Imports` evidence: a linked local stylesheet/script names the
//! target file's `Module` symbol, pinned to its path and language.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::{LanguageParser, LanguageRegistry, RelationKind, SourceFile, SymbolKind};
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_css::CssParser;
use mct_lang_html::HtmlParser;

#[test]
fn a_linked_stylesheet_resolves_to_the_css_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/site");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(HtmlParser));
    registry.register(Arc::new(CssParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let hit = index
        .find_references("style")
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "imports" && h.relative_path == "index.html")
        .unwrap();
    assert_eq!(hit.resolution, Resolution::Resolved);
    assert_eq!(hit.candidates[0].relative_path, "style.css");
}

#[test]
fn evidence_is_set_only_where_the_source_proves_it() {
    let parsed = HtmlParser
        .parse(&SourceFile {
            relative_path: "pages/index.html".to_string(),
            contents: concat!(
                "<html><head>",
                "<link rel=\"stylesheet\" href=\"../css/site.css?v=2\">",
                "<link rel=\"stylesheet\" href=\"https://cdn.example.com/lib.css\">",
                "<script src=\"app.js\"></script>",
                "<script src=\"data.json\"></script>",
                "</head></html>"
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
    let css = evidence("site").unwrap();
    assert_eq!(css.path.as_deref(), Some("css/site.css"));
    assert_eq!(css.language.as_deref(), Some("css"));
    assert_eq!(css.kind, Some(SymbolKind::Module));
    let js = evidence("app").unwrap();
    assert_eq!(js.path.as_deref(), Some("pages/app.js"));
    assert_eq!(js.language.as_deref(), Some("javascript_typescript"));
    // An absolute URL is provably outside the repository.
    assert!(
        evidence("https://cdn.example.com/lib.css")
            .unwrap()
            .external
    );
    // A file that is no stylesheet or script gets only the plain relation.
    assert!(evidence("data.json").is_none());
    assert!(evidence("data").is_none());
}
