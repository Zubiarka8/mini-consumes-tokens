//! Tailwind application CSS coverage through the shared CSS parser.

use mct_core::RelationKind;

use crate::libraries::{corpus, rules};

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
fn tailwind_v3_input_layers_apply_and_escaped_overrides() {
    let path = "tailwind-v3-app/src/input.css";
    // The input file imports nothing: `@tailwind` is not `@import`.
    assert!(corpus().relations().iter().all(|r| r.path != path));
    // Rules inside `@layer components { … }` keep their block, `@apply`
    // lines included.
    assert_eq!(rules(path, ".btn-primary"), [(54, 57), (143, 146)]);
    assert_eq!(rules(path, ".ledger-card"), [(63, 66), (68, 68)]);
    assert_eq!(
        rules(path, ".ledger-sidebar"),
        [(118, 120), (124, 126), (142, 146)]
    );
    // `@tailwind`, `@layer` names, `@apply` arguments and `theme()` paths
    // are not rules.
    assert_no_symbols(
        path,
        &[
            "base",
            "components",
            "utilities",
            "text-2xl",
            "colors.slate.900",
        ],
    );
    // Escaped utility names are indexed as the HTML class reads.
    assert_eq!(rules(path, ".md:flex"), [(134, 136)]);
}

#[test]
fn tailwind_v3_hex_escaped_class_is_indexed_as_the_html_class() {
    let path = "tailwind-v3-app/src/input.css";
    // `.\33xl\:grid-cols-6` is the class `3xl:grid-cols-6` (CSS Syntax 3
    // §4.3.7: `\33` is the code point U+0033, `3`).
    assert_eq!(rules(path, ".3xl:grid-cols-6"), [(138, 140)]);
    assert!(rules(path, ".33xl:grid-cols-6").is_empty());
}

#[test]
fn tailwind_v4_theme_utilities_variants_and_nesting() {
    let path = "tailwind-v4-app/src/main.css";
    let r = corpus().relation(path, "main", RelationKind::Imports, "tailwindcss");
    assert_eq!(r.line, 25);
    // `@theme` (tokens and the keyframes inside it), `@layer` lists and
    // `@custom-variant` names produce no rules.
    assert_no_symbols(
        path,
        &[
            "theme",
            "inline",
            "wiggle",
            "fade-in",
            "--font-display",
            "0%",
            "theme-midnight",
            "--hero-angle",
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
    assert_eq!(rules(path, "&::-webkit-scrollbar"), [(121, 123)]);
    // A rule spans its nested rules and `@variant` blocks…
    assert_eq!(rules(path, ".hb-card"), [(166, 183)]);
    assert_eq!(rules(path, ".hero"), [(256, 268)]);
    assert_eq!(rules(path, ".pricing-table"), [(275, 287)]);
    // …and, known limit, each nested rule is a separate rule named with
    // its literal `&` selector, not resolved against the parent (#115).
    assert!(rules(path, "&:hover").contains(&(170, 172)));
    assert_eq!(rules(path, "& > .hb-card__title"), [(174, 177)]);
    assert_eq!(rules(path, ".hb-card__title"), [(174, 174)]);
}
