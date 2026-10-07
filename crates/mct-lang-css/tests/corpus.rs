//! Long, multi-file fixture corpus (issue #74): the Harbor UI kit — design
//! tokens, a base layer, the page layout, two component files and the
//! dashboard's `app.css` that `@import`s them all — each 300–600 lines. The
//! shared checks (size, line ranges, golden snapshot, index round trip,
//! malformed input) come from `mct-corpus`; the tests below pin the
//! constructs and relations this language is expected to extract.
//!
//! `frameworks/` (issue #114) adds Tailwind v3 and v4 stylesheets (separate
//! files: their directives differ) and Bootstrap 5.3-shaped compiled CSS.

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
    // Imports are the only relation CSS produces: 12 in the Harbor kit,
    // 3 in `frameworks/` (Tailwind's `@import "tailwindcss"` and two of
    // `../styles/tokens.css`).
    assert!(c
        .relations()
        .iter()
        .all(|r| r.kind == RelationKind::Imports));
    assert_eq!(c.relations().len(), 15);
}

/// Fails if any of `names` is a symbol of the file ending in `path`.
fn assert_no_symbols(path: &str, names: &[&str]) {
    for name in names {
        let hits: Vec<_> = corpus()
            .symbols_named(name)
            .into_iter()
            .filter(|(p, _)| p.ends_with(path))
            .collect();
        assert!(hits.is_empty(), "`{name}` should not be a symbol: {hits:?}");
    }
}

#[test]
fn tailwind_v3_directives_layers_and_escaped_utilities() {
    let path = "frameworks/tailwind-v3.css";
    corpus().relation(
        path,
        "tailwind-v3",
        RelationKind::Imports,
        "../styles/tokens.css",
    );
    // Rules inside `@layer components { … }` keep their block, `@apply`
    // lines included; `@screen md`/`@screen lg` wrap rules like `@media`.
    assert_eq!(rules(path, ".btn-primary"), [(50, 53)]);
    assert_eq!(rules(path, ".tw-sidebar"), [(106, 108), (112, 114)]);
    // `@tailwind`, `@apply` arguments and `theme()` paths are not rules.
    assert_no_symbols(
        path,
        &[
            "base",
            "components",
            "utilities",
            "variants",
            "md",
            "tw-spin",
        ],
    );
    // Escaped utilities (variants, fractions, arbitrary values, the `!`
    // modifier, arbitrary properties) are indexed as the HTML class reads.
    assert_eq!(rules(path, ".md:flex"), [(282, 284)]);
    assert_eq!(rules(path, ".w-1/2"), [(149, 151)]);
    assert_eq!(rules(path, ".!mt-0"), [(161, 163)]);
    assert_eq!(rules(path, ".w-[32rem]"), [(165, 167)]);
    assert_eq!(rules(path, ".bg-[#1da1f2]"), [(169, 172)]);
    assert_eq!(rules(path, ".[mask-type:luminance]"), [(178, 180)]);
    assert_eq!(rules(path, ".lg:grid-cols-[1fr_2fr]"), [(296, 298)]);
    assert_eq!(rules(path, ".max-md:hidden"), [(307, 309)]);
    assert_eq!(rules(path, ".supports-[display:grid]:grid"), [(331, 333)]);
    // Variant atoms in compound and combinator selectors.
    assert!(rules(path, ".data-[state=open]:block").contains(&(251, 251)));
    assert!(rules(path, ".group-hover/item:visible").contains(&(233, 233)));
    assert!(rules(path, ".peer-checked:bg-sky-600").contains(&(237, 237)));
    assert!(rules(path, ".dark:bg-slate-900").contains(&(261, 261)));
}

#[test]
fn tailwind_v4_theme_utilities_variants_and_nesting() {
    let path = "frameworks/tailwind-v4.css";
    let r = corpus().relation(path, "tailwind-v4", RelationKind::Imports, "tailwindcss");
    assert_eq!(r.line, 20);
    // `@theme` (tokens and the keyframes inside it) and `@layer` lists
    // produce no rules.
    assert_no_symbols(
        path,
        &[
            "theme",
            "inline",
            "wiggle",
            "fade-in",
            "--font-display",
            "0%",
        ],
    );
    // Known limit: `@utility name { … }` declares a class the HTML can
    // use, but the name is not indexed; only its nested rule is.
    assert_no_symbols(
        path,
        &[
            "tab-4",
            ".tab-4",
            "btn",
            "scrollbar-hidden",
            ".scrollbar-hidden",
        ],
    );
    assert_eq!(rules(path, "&::-webkit-scrollbar"), [(118, 120)]);
    // Block-form `@custom-variant` with `@slot`.
    assert_eq!(
        rules(path, "&:where([data-theme=\"midnight\"] *)"),
        [(84, 86), (307, 309)]
    );
    // A rule spans its nested rules and `@variant` blocks…
    assert_eq!(rules(path, ".hb-card"), [(156, 173)]);
    // …and, known limit, each nested rule is a separate rule named with
    // its literal `&` selector, not resolved against the parent (#115).
    assert!(rules(path, "&:hover").contains(&(160, 162)));
    assert_eq!(rules(path, "& > .hb-card__title"), [(164, 167)]);
    assert_eq!(rules(path, ".hb-card__title"), [(164, 164)]);
    assert_eq!(rules(path, ".@container"), [(318, 320)]);
    assert_eq!(rules(path, ".@md:flex-row"), [(325, 327)]);
    assert_eq!(rules(path, ".bg-harbor-500/50"), [(268, 270)]);
    // `\33xl\:…` is the class `3xl:grid-cols-6` (hex escape `\33` + space).
    assert_eq!(rules(path, ".3xl:grid-cols-6"), [(288, 290)]);
    assert!(rules(path, ".33xl:grid-cols-6").is_empty());
}

#[test]
fn bootstrap_color_modes_grid_and_state_selectors() {
    let path = "frameworks/bootstrap.css";
    // `:root, [data-bs-theme=light]` share one block of `--bs-*` tokens.
    assert_eq!(rules(path, ":root"), [(13, 39), (69, 71)]);
    assert_eq!(rules(path, "[data-bs-theme=light]"), [(14, 39)]);
    // The dark color mode: its own block, and as an ancestor atom.
    assert_eq!(rules(path, "[data-bs-theme=dark]"), [(41, 48), (240, 240)]);
    assert_eq!(
        rules(path, "[data-bs-theme=dark] .alert-primary"),
        [(240, 243)]
    );
    // Grid rules inside `@media (min-width: …)`.
    assert_eq!(rules(path, ".col-md-6"), [(109, 112)]);
    assert_eq!(
        rules(path, ".container"),
        [(88, 96), (99, 101), (105, 107), (125, 127)]
    );
    // State selectors with sibling combinators and chained `:not()`.
    assert_eq!(rules(path, ".btn-check:checked + .btn"), [(192, 198)]);
    assert!(rules(path, ".btn").contains(&(192, 192)));
    assert_eq!(
        rules(
            path,
            ".visually-hidden-focusable:not(:focus):not(:focus-within)"
        ),
        [(307, 314)]
    );
    assert_no_symbols(path, &["progress-bar-stripes", "0%"]);
    // A compiled bundle has no imports.
    assert!(corpus().relations().iter().all(|r| r.path != path));
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
            "frameworks/bootstrap.css",
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
