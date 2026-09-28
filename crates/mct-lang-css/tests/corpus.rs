//! Long, multi-file fixture corpus (issue #74): the Harbor UI kit — design
//! tokens, a base layer, the page layout, two component files and the
//! dashboard's `app.css` that `@import`s them all — each 300–600 lines. The
//! shared checks (size, line ranges, golden snapshot, index round trip,
//! malformed input) come from `mct-corpus`; the tests below pin the
//! constructs and relations this language is expected to extract.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-css/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_css::CssParser;

mct_corpus::standard_tests!(CssParser);

/// Every `(line, end_line)` of the `Rule` symbols named `name` in `path`,
/// in file order — a selector may be written many times in one file.
fn rules(path: &str, name: &str) -> Vec<(u32, u32)> {
    let mut out: Vec<_> = corpus()
        .symbols_named(name)
        .into_iter()
        .filter(|(p, s)| p.ends_with(path) && s.kind == SymbolKind::Rule)
        .map(|(_, s)| {
            let line = s.location.line;
            (line, s.location.end_line.unwrap_or(line))
        })
        .collect();
    out.sort_unstable();
    out
}

#[test]
fn a_rule_spans_its_declaration_block() {
    // `:root { … }` with 55 custom properties.
    assert_eq!(rules("styles/tokens.css", ":root")[0], (67, 126));
    // Inside `@media (prefers-color-scheme: dark) { … }`.
    assert_eq!(
        rules("styles/tokens.css", ":root:not([data-theme=\"light\"])"),
        [(131, 150)]
    );
    // Every selector of a comma-separated list runs from its own line to
    // the shared closing brace.
    assert_eq!(rules("styles/base.css", "body")[0], (26, 37));
    assert_eq!(rules("styles/base.css", "h1")[0], (27, 37));
    assert_eq!(rules("styles/base.css", "dd")[0], (35, 37));
    assert_eq!(rules("styles/components/controls.css", ".btn")[0], (12, 36));
    // The file-level module ends on the file's last line.
    let tokens = corpus().symbol("styles/tokens.css", "tokens", SymbolKind::Module);
    assert_eq!(
        (tokens.location.line, tokens.location.end_line),
        (1, Some(353))
    );
    let app = corpus().symbol("app.css", "app", SymbolKind::Module);
    assert_eq!((app.location.line, app.location.end_line), (1, Some(361)));
}

#[test]
fn compound_selectors_add_their_atoms_on_the_selector_line() {
    let path = "styles/components/controls.css";
    assert_eq!(rules(path, "input[type=\"search\"].input")[0], (217, 222));
    // The atoms keep their own one-token range.
    assert!(rules(path, "input").contains(&(217, 217)));
    assert!(rules(path, "input[type=\"search\"]").contains(&(217, 217)));
    assert!(rules(path, ".input").contains(&(217, 217)));
    // `.kpi.card` is `.kpi` and `.card` too.
    assert_eq!(rules("app.css", ".kpi.card"), [(92, 94)]);
    assert!(rules("app.css", ".kpi").contains(&(92, 92)));
    assert!(rules("app.css", ".card").contains(&(92, 92)));
    // The grammar applies `[attr]` to the whole descendant chain before
    // it; every atom of that chain is still extracted.
    let path = "app.css";
    let full = ".dashboard__toolbar .btn-group .btn[aria-pressed=\"true\"]";
    assert_eq!(rules(path, full), [(75, 78)]);
    for atom in [".dashboard__toolbar", ".btn-group", ".btn"] {
        assert!(rules(path, atom).contains(&(75, 75)), "{atom}");
    }
    assert!(rules(path, ".data-table").contains(&(223, 223)));
    // A pseudo-class with selector arguments keeps its full text.
    assert_eq!(
        rules(
            "app.css",
            ".onboarding__step:is(.is-current, :focus-within)"
        ),
        [(320, 322)]
    );
}

#[test]
fn escaped_class_names_are_unescaped() {
    // `.md\:hidden` in `@layer utilities` and again in `@media`.
    assert_eq!(
        rules("styles/base.css", ".md:hidden"),
        [(336, 338), (354, 356)]
    );
    assert_eq!(rules("styles/base.css", ".w-1/2"), [(340, 342)]);
    assert!(corpus().symbols_named(".md\\:hidden").is_empty());
}

#[test]
fn a_namespace_prefix_is_not_an_element() {
    // `.chart svg|text` (after `@namespace svg url(…)`) selects `<text>`.
    assert_eq!(rules("app.css", ".chart svg|text"), [(150, 153)]);
    assert_eq!(rules("app.css", "text"), [(150, 150)]);
    assert!(rules("app.css", "svg").is_empty());
    // The only `svg` rule is the real element in the reset.
    let svg: Vec<_> = corpus()
        .symbols_named("svg")
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert_eq!(svg, ["styles/base.css"]);
}

#[test]
fn rules_inside_conditional_at_rules_are_extracted() {
    // `@container (min-width: …)` twice.
    assert_eq!(
        rules("styles/layout.css", ".panel__body"),
        [(281, 285), (293, 295)]
    );
    // `@supports not (display: grid)`.
    assert!(rules("styles/layout.css", ".grid").contains(&(230, 233)));
    assert_eq!(rules("styles/layout.css", ".grid > *"), [(235, 238)]);
    // `@media (min-width: 64em)`.
    assert!(rules("styles/layout.css", ".app-shell").contains(&(102, 108)));
    // `@layer reset { … }`.
    assert_eq!(rules("styles/base.css", "*::before"), [(15, 18)]);
}

#[test]
fn at_rule_preludes_and_keyframe_steps_are_not_rules() {
    let c = corpus();
    for name in [
        "from",
        "to",
        "0%",
        "50%",
        "100%",
        "harbor-spin",
        "@keyframes",
        "@font-face",
        "@property",
        "--harbor-progress",
        "Inter",
    ] {
        assert!(
            c.symbols_named(name).is_empty(),
            "`{name}` should not be a symbol: {:?}",
            c.symbols_named(name)
        );
    }
    // tokens.css is almost all at-rules and `:root`: only these selectors.
    let names: Vec<_> = c
        .files
        .iter()
        .filter(|f| f.path == "styles/tokens.css")
        .flat_map(|f| f.parsed.symbols.iter())
        .filter(|s| s.kind == SymbolKind::Rule && s.name != ":root")
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            ":root:not([data-theme=\"light\"])",
            ":root[data-theme=\"dark\"]",
            ".theme-dark",
            ".theme-light",
        ]
    );
}

#[test]
fn every_import_form_is_an_import_of_the_path_as_written() {
    let c = corpus();
    // `@import "…";`
    let r = c.relation("app.css", "app", RelationKind::Imports, "styles/tokens.css");
    assert_eq!(r.line, 8);
    // `@import url("…");`
    c.relation("app.css", "app", RelationKind::Imports, "styles/layout.css");
    // `@import url("…") screen;` — the media query is not part of the path.
    c.relation(
        "app.css",
        "app",
        RelationKind::Imports,
        "styles/components/surfaces.css",
    );
    // `@import url(base.css);` — unquoted.
    c.relation(
        "styles/layout.css",
        "layout",
        RelationKind::Imports,
        "base.css",
    );
    // `@import "../base.css" screen;` and `./` paths are kept verbatim.
    c.relation(
        "styles/components/controls.css",
        "controls",
        RelationKind::Imports,
        "../base.css",
    );
    c.relation(
        "styles/components/surfaces.css",
        "surfaces",
        RelationKind::Imports,
        "./controls.css",
    );
    // `@namespace svg url(…)` is not an import.
    assert!(!c.has_relation("app", RelationKind::Imports, "http://www.w3.org/2000/svg"));
    // Imports are the only relation CSS produces.
    assert!(c
        .relations()
        .iter()
        .all(|r| r.kind == RelationKind::Imports));
    assert_eq!(c.relations().len(), 12);
}

#[test]
fn index_finds_a_class_in_every_file_that_styles_it() {
    let index = corpus().index();
    let mut files: Vec<_> = index
        .find_symbol(".btn")
        .unwrap()
        .into_iter()
        .map(|h| h.relative_path)
        .collect();
    files.sort();
    files.dedup();
    assert_eq!(
        files,
        [
            "app.css",
            "styles/components/controls.css",
            "styles/components/surfaces.css",
        ]
    );
    let card: Vec<_> = index
        .find_symbol(".card")
        .unwrap()
        .into_iter()
        .map(|h| (h.relative_path, h.line, h.end_line))
        .collect();
    assert!(card.contains(&("styles/components/surfaces.css".to_string(), 12, Some(20))));
    assert!(card.contains(&("app.css".to_string(), 92, Some(92))));
    let imports = index.find_references("styles/base.css").unwrap();
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].relative_path, "app.css");
    assert_eq!(imports[0].line, 9);
}
