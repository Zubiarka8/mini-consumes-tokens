//! A custom property declaration (`--brand: red`) and a `@keyframes` rule
//! are definitions, and a stylesheet reading them refers to them, through
//! the real parse -> SQLite -> query pipeline: `var(--brand)` references
//! the `--brand` variable, a fallback's `var(--accent)` too, and each
//! animation name in `animation`/`animation-name` references its
//! `@keyframes`. A keyword (`infinite`, `ease-in-out`), a selector of the
//! same spelling, and a property that names no animation reference
//! nothing.
//!
//! The project is written to a temporary directory, so its names never
//! become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_core::{RelationKind, SymbolKind};
use mct_index::{Index, Resolution};
use mct_lang_css::CssParser;

const THEME: &str = "\
:root { --brand: red; --accent: blue; }
.dark { --brand: black; }
@keyframes spin { from { opacity: 0 } to { opacity: var(--brand) } }
@keyframes fade { to { opacity: 0 } }
";

const APP: &str = "\
.btn { color: var(--brand); border-color: var(--missing, var(--accent)); }
.spinner { animation: spin 1s ease-in-out infinite; }
.toast { animation-name: fade, spin; transition: fade 1s; }
.fade { color: red; }
";

fn parsed(source: &str) -> (Vec<String>, Vec<String>) {
    let parsed = mct_corpus::parse_source(&CssParser, "app.css", "CSS", source);
    let symbols = parsed
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Variable | SymbolKind::Animation))
        .map(|s| format!("{} {:?} {}", s.location.line, s.kind, s.name))
        .collect();
    let references = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::References)
        .map(|(i, r)| {
            let kind = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.kind);
            format!("{} {} {kind:?}", r.location.line, r.to_name)
        })
        .collect();
    (symbols, references)
}

#[test]
fn custom_properties_and_keyframes_are_definitions() {
    let (symbols, references) = parsed(THEME);
    assert_eq!(
        symbols,
        [
            "1 Variable --brand",
            "1 Variable --accent",
            "2 Variable --brand",
            "3 Animation spin",
            "4 Animation fade",
        ]
    );
    assert_eq!(references, ["3 --brand Some(Variable)"]);
}

#[test]
fn var_and_animation_values_reference_them() {
    let (symbols, references) = parsed(APP);
    assert!(symbols.is_empty(), "{symbols:?}");
    assert_eq!(
        references,
        [
            "1 --brand Some(Variable)",
            "1 --missing Some(Variable)",
            "1 --accent Some(Variable)",
            "2 spin Some(Animation)",
            "3 fade Some(Animation)",
            "3 spin Some(Animation)",
        ]
    );
}

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-css-custom-properties-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("theme.css"), THEME).unwrap();
    std::fs::write(root.join("app.css"), APP).unwrap();
    let index = mct_corpus::open_indexed(&root, Arc::new(CssParser), 2, "theme.css, app.css");
    (index, root)
}

#[test]
fn an_animation_name_resolves_to_its_keyframes_not_a_same_named_rule() {
    let (index, root) = indexed();
    let hits: Vec<_> = index
        .find_references("fade")
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == "app.css")
        .collect();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].line, 3);
    assert_eq!(hits[0].resolution, Resolution::Resolved, "{hits:?}");
    let target = index.symbols_by_ids(&[hits[0].target_id.unwrap()]).unwrap();
    assert_eq!(
        (target[0].relative_path.as_str(), target[0].line),
        ("theme.css", 4)
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_var_read_finds_every_declaration_of_its_property() {
    let (index, root) = indexed();
    let accent: Vec<_> = index
        .find_references("--accent")
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == "app.css")
        .collect();
    assert_eq!(accent.len(), 1, "{accent:?}");
    assert_eq!(accent[0].resolution, Resolution::Resolved, "{accent:?}");
    // `--brand` is declared twice (`:root` and `.dark`): which one applies
    // depends on the element, so the read stays ambiguous between both.
    let brand: Vec<_> = index
        .find_references("--brand")
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == "app.css")
        .collect();
    assert_eq!(brand.len(), 1, "{brand:?}");
    assert_eq!(brand[0].resolution, Resolution::Ambiguous, "{brand:?}");
    assert!(index.find_symbol("--missing").unwrap().is_empty());
    std::fs::remove_dir_all(root).ok();
}
