//! Long, multi-file fixture corpus (issue #74): the public shop of a
//! warehouse — home, catalog, product, cart/checkout and account pages, each
//! 300–600 lines. They share stylesheets and scripts, repeat the same header,
//! cart drawer and footer ids on every page, and link to each other's
//! fragments (`catalog.html#filters`, `account.html#orders`). The shared
//! checks (size, line ranges, golden snapshot, index round trip, malformed
//! input) come from `mct-corpus`; the tests below pin what HTML is expected
//! to extract and its documented limits.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-html/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_html::HtmlParser;

mct_corpus::standard_tests!(HtmlParser);

fn parent<'a>(path: &str, id: &str) -> Option<&'a str> {
    corpus()
        .symbol(path, id, SymbolKind::Element)
        .parent
        .as_deref()
}

fn imports(path: &str, from: &str, href: &str) {
    corpus().relation(path, from, RelationKind::Imports, href);
}

#[test]
fn elements_with_an_id_are_symbols_owned_by_the_nearest_id_ancestor() {
    assert_eq!(parent("index.html", "home"), None);
    assert_eq!(parent("index.html", "site-header"), Some("home"));
    assert_eq!(parent("index.html", "logo"), Some("site-header"));
    // Through elements without an id (`<div class="container">`, `<li>`).
    assert_eq!(parent("index.html", "search-input"), Some("search-form"));
    assert_eq!(parent("index.html", "cart-count"), Some("cart-toggle"));
    assert_eq!(parent("product.html", "lightbox-video"), Some("gallery-lightbox"));
    assert_eq!(parent("cart.html", "qty-2"), Some("line-2"));
    assert_eq!(parent("account.html", "orders-year"), Some("orders-filter"));
    // `<template>` content and inline `<svg>` are walked like any element.
    assert_eq!(parent("catalog.html", "recently-viewed-item"), Some("recently-viewed"));
    assert_eq!(parent("index.html", "step-ship"), Some("how-it-works"));
}

#[test]
fn id_and_class_tokens_reference_their_selectors() {
    let c = corpus();
    c.relation("index.html", "hero", RelationKind::References, "#hero");
    c.relation("index.html", "hero", RelationKind::References, ".hero");
    c.relation("index.html", "hero", RelationKind::References, ".hero--split");
    // Every whitespace-separated class is its own reference.
    for class in [".product-card", ".product-card--sale"] {
        c.relation("catalog.html", "product-sku-1001", RelationKind::References, class);
    }
}

#[test]
fn classes_without_an_id_are_the_documented_limit() {
    let c = corpus();
    // `<a class="skip-link">`, `<nav class="breadcrumbs">` and the card links
    // have no id, so their classes are never referenced.
    for class in [".skip-link", ".breadcrumbs", ".product-card__link"] {
        assert!(
            !c.relations().iter().any(|r| r.to == class),
            "{class} was referenced"
        );
    }
}

#[test]
fn stylesheets_and_scripts_are_imports() {
    // From the file module when no id element encloses them…
    imports("index.html", "index", "assets/css/site.css");
    imports("index.html", "index", "assets/css/print.css");
    imports("index.html", "index", "assets/js/app.js");
    imports("cart.html", "cart", "https://js.payments.example/v3/");
    // …from the element itself when the script has an id…
    imports("product.html", "gallery-script", "assets/js/gallery.js");
    // …and from the id ancestor when a script sits inside one.
    imports("index.html", "home", "assets/js/home.js");
    // Upper-case and unquoted markup.
    imports("catalog.html", "catalog", "assets/css/legacy-filters.css");
    imports("product.html", "product", "assets/css/gallery.css");
}

#[test]
fn other_links_and_inline_code_are_not_imports() {
    let c = corpus();
    let imported: Vec<_> = c
        .relations()
        .into_iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to)
        .collect();
    for not_imported in [
        // `rel="alternate stylesheet"`, preload, icon and manifest links.
        "assets/css/high-contrast.css",
        "assets/fonts/inter-var.woff2",
        "assets/img/favicon.svg",
        "manifest.webmanifest",
        // Anchors and images.
        "catalog.html",
        "assets/img/hero.jpg",
        // An empty `href=""`.
        "",
    ] {
        assert!(!imported.contains(&not_imported), "{not_imported:?} imported");
    }
}

#[test]
fn shared_ids_repeat_once_per_page() {
    let pages: Vec<_> = corpus()
        .symbols_named("site-header")
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    assert_eq!(
        pages,
        ["account.html", "cart.html", "catalog.html", "index.html", "product.html"]
    );
}

#[test]
fn style_and_script_elements_with_an_id_are_symbols() {
    assert_eq!(parent("index.html", "critical-css"), None);
    corpus().symbol("product.html", "gallery-script", SymbolKind::Element);
}

#[test]
fn index_answers_selector_and_asset_queries() {
    let index = corpus().index();
    let refs = index.find_references("#site-header").unwrap();
    assert_eq!(refs.len(), 5);
    let refs = index.find_references("assets/js/app.js").unwrap();
    let mut pages: Vec<_> = refs.iter().map(|r| r.relative_path.as_str()).collect();
    pages.sort();
    assert_eq!(
        pages,
        ["account.html", "cart.html", "catalog.html", "index.html", "product.html"]
    );
    let refs = index.find_references(".add-to-cart").unwrap();
    assert!(refs.is_empty(), "the add-to-cart buttons have no id");
}
