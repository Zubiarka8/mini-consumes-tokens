//! Bootstrap application CSS coverage through the shared CSS parser.

use mct_core::RelationKind;

use crate::libraries::{corpus, rules};

#[test]
fn bootstrap_shop_customizes_a_versioned_external_bootstrap() {
    let c = corpus();
    // Bootstrap is imported from its versioned CDN build, as written; the
    // shop's partials follow.
    for (line, target) in [
        (
            19,
            "https://cdn.jsdelivr.net/npm/bootstrap@5.3.8/dist/css/bootstrap.min.css",
        ),
        (20, "theme.css"),
        (21, "catalog.css"),
        (22, "checkout.css"),
    ] {
        let r = c.relation(
            "bootstrap-shop/shop.css",
            "shop",
            RelationKind::Imports,
            target,
        );
        assert_eq!(r.line, line, "{target}");
    }
    // No Bootstrap rule is in the corpus: `.btn` and `.card` only appear
    // as atoms of the shop's own selectors.
    assert!(rules("bootstrap-shop/catalog.css", ".btn")
        .iter()
        .all(|(a, b)| a == b));
    assert_eq!(rules("bootstrap-shop/catalog.css", ".card"), [(104, 104)]);
    // A button variant built from `--bs-btn-*` variables, and its size
    // modifier as a compound selector.
    assert_eq!(
        rules("bootstrap-shop/catalog.css", ".btn-brand"),
        [(11, 23), (25, 25), (179, 179)]
    );
    assert_eq!(
        rules("bootstrap-shop/catalog.css", ".btn-brand.btn-lg"),
        [(25, 28)]
    );
    // The custom color mode spans its nested component overrides, which
    // are rules of their own.
    let theme = "bootstrap-shop/theme.css";
    assert_eq!(rules(theme, "[data-bs-theme=\"harbor-night\"]"), [(40, 60)]);
    assert_eq!(rules(theme, ".dropdown-menu"), [(51, 54)]);
    assert_eq!(rules(theme, ".btn-brand"), [(56, 59)]);
    assert_eq!(rules(theme, ":root"), [(10, 29)]);
    // An id selector with a descendant: the rule and its `#id` atom.
    assert_eq!(
        rules("bootstrap-shop/catalog.css", "#quick-view .modal-content"),
        [(187, 190)]
    );
}
