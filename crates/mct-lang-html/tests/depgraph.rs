//! File-level `Imports` evidence: a linked local stylesheet/script names the
//! target file's `Module` symbol, pinned to its path and language.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::{LanguageParser, LanguageRegistry, SourceFile, SymbolKind};
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
    let evidence = |name: &str| mct_corpus::optional_import_evidence(&parsed, name);
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

fn evidence_in(path: &str, html: &str, name: &str) -> Option<mct_core::RelationTarget> {
    let parsed = HtmlParser
        .parse(&SourceFile {
            relative_path: path.to_string(),
            contents: html.to_string(),
        })
        .unwrap();
    mct_corpus::optional_import_evidence(&parsed, name)
}

/// Issue #567: a link is a URL, so `\` is a separator and `%XX` is decoded
/// once before the path resolves.
#[test]
fn a_link_with_backslashes_or_escapes_resolves_like_a_url() {
    let css = evidence_in(
        "site/pages/index.html",
        r#"<link rel="stylesheet" href="..\css\site.css">"#,
        "site",
    )
    .unwrap();
    assert_eq!(css.path.as_deref(), Some("site/css/site.css"));
    let js = evidence_in(
        "site/index.html",
        r#"<script src="my%20app.js"></script>"#,
        "my app",
    )
    .unwrap();
    assert_eq!(js.path.as_deref(), Some("site/my app.js"));
}

/// Issue #567: `<base href>` is the base of every relative link after it;
/// a root-relative or absolute link ignores it, and a base on another host
/// leaves relative links with no local file.
#[test]
fn a_relative_link_resolves_against_the_base_href() {
    let page = |base: &str| {
        format!(
            r#"<html><head><base href="{base}"><link rel="stylesheet" href="css/a.css"><link rel="stylesheet" href="./b.css"></head></html>"#
        )
    };
    let path = |base: &str, name: &str| {
        evidence_in("site/docs/index.html", &page(base), name).and_then(|e| e.path)
    };
    assert_eq!(path("../", "a").as_deref(), Some("site/css/a.css"));
    assert_eq!(
        path("shared/", "b").as_deref(),
        Some("site/docs/shared/b.css")
    );
    // A base file name is not a directory: `x/index.html` bases `x/`.
    assert_eq!(
        path("x/index.html", "a").as_deref(),
        Some("site/docs/x/css/a.css")
    );
    assert_eq!(path("https://cdn.example.com/app/", "a"), None);
    let root = evidence_in(
        "site/docs/index.html",
        r#"<base href="../"><link rel="stylesheet" href="https://cdn.example.com/c.css">"#,
        "https://cdn.example.com/c.css",
    )
    .unwrap();
    assert!(root.external, "an absolute link ignores the base");
}

/// Issue #567: a link that climbs above the repository root names a file
/// outside it. It is `external`, with no path stored, not merely unresolved.
#[test]
fn a_link_leaving_the_repository_is_external_with_no_path() {
    let spec = "../../other-repo/x.css";
    let outside = evidence_in(
        "web/index.html",
        &format!(r#"<link rel="stylesheet" href="{spec}">"#),
        spec,
    )
    .unwrap();
    assert!(outside.external);
    assert_eq!(outside.path, None);
    assert!(evidence_in(
        "web/index.html",
        &format!(r#"<link rel="stylesheet" href="{spec}">"#),
        "x"
    )
    .is_none());
    // One level up from `web/` is still the repository root.
    assert!(evidence_in(
        "web/index.html",
        r#"<link rel="stylesheet" href="../x.css">"#,
        "../x.css"
    )
    .is_none());
}
