// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-css/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_css::CssParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    CssParser
        .parse(&SourceFile {
            relative_path: "style.css".to_string(),
            contents: src.to_string(),
        })
        .expect("valid CSS source should parse")
}

#[test]
fn extracts_simple_class_selector() {
    let parsed = parse(".nav { display: flex; }");
    let rule = parsed.symbols.iter().find(|s| s.name == ".nav").unwrap();
    assert_eq!(rule.kind, SymbolKind::Rule);
}

#[test]
fn extracts_simple_id_selector() {
    let parsed = parse("#header { color: navy; }");
    let rule = parsed.symbols.iter().find(|s| s.name == "#header").unwrap();
    assert_eq!(rule.kind, SymbolKind::Rule);
}

#[test]
fn comma_separated_selectors_each_become_their_own_rule() {
    let parsed = parse(".a, .b { color: red; }");
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == ".a" && s.kind == SymbolKind::Rule));
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == ".b" && s.kind == SymbolKind::Rule));
}

fn rule_names(parsed: &mct_core::ParsedFile) -> Vec<&str> {
    parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Rule)
        .map(|s| s.name.as_str())
        .collect()
}

#[test]
fn compound_selector_indexes_full_and_atomic_names() {
    let parsed = parse("div.card { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&"div.card"), "{names:?}");
    assert!(names.contains(&"div"), "{names:?}");
    assert!(names.contains(&".card"), "{names:?}");
}

#[test]
fn compound_class_selector_indexes_full_and_atomic_names() {
    let parsed = parse(".btn.btn-primary { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&".btn.btn-primary"), "{names:?}");
    assert!(names.contains(&".btn"), "{names:?}");
    assert!(names.contains(&".btn-primary"), "{names:?}");
}

#[test]
fn descendant_combinator_indexes_full_and_atomic_names() {
    let parsed = parse(".card .title { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&".card .title"), "{names:?}");
    assert!(names.contains(&".card"), "{names:?}");
    assert!(names.contains(&".title"), "{names:?}");
}

#[test]
fn child_combinator_indexes_full_and_atomic_names() {
    let parsed = parse("#id > .c { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&"#id > .c"), "{names:?}");
    assert!(names.contains(&"#id"), "{names:?}");
    assert!(names.contains(&".c"), "{names:?}");
}

#[test]
fn bare_tag_selector_is_indexed() {
    let parsed = parse("div { color: red; }");
    assert_eq!(rule_names(&parsed), vec!["div"]);
}

#[test]
fn pseudo_class_selector_indexes_full_and_base_names() {
    let parsed = parse(".btn:hover { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&".btn:hover"), "{names:?}");
    assert!(names.contains(&".btn"), "{names:?}");
}

#[test]
fn pseudo_element_selector_is_indexed_alone() {
    let parsed = parse("::before { color: red; }");
    assert_eq!(rule_names(&parsed), vec!["::before"]);
}

#[test]
fn attribute_selector_is_indexed() {
    let parsed = parse("[data-bs-toggle] { color: red; }");
    assert_eq!(rule_names(&parsed), vec!["[data-bs-toggle]"]);
}

#[test]
fn tag_plus_attribute_selector_indexes_full_and_atomic_names() {
    let parsed = parse("input[type=\"text\"] { color: red; }");
    let names = rule_names(&parsed);
    assert!(names.contains(&"input[type=\"text\"]"), "{names:?}");
    assert!(names.contains(&"input"), "{names:?}");
}

#[test]
fn escaped_tailwind_class_names_are_unescaped() {
    let parsed = parse(".md\\:flex { display: flex; }");
    assert_eq!(rule_names(&parsed), vec![".md:flex"]);

    let parsed = parse(".hover\\:bg-blue-500 { color: blue; }");
    assert_eq!(rule_names(&parsed), vec![".hover:bg-blue-500"]);

    let parsed = parse(".w-1\\/2 { width: 50%; }");
    assert_eq!(rule_names(&parsed), vec![".w-1/2"]);
}

#[test]
fn rules_nested_in_a_media_query_are_still_indexed() {
    let parsed = parse("@media (min-width: 600px) { .nav { display: flex; } }");
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == ".nav" && s.kind == SymbolKind::Rule));
}

#[test]
fn at_import_with_a_plain_string_creates_an_import_relation() {
    let parsed = parse("@import \"reset.css\";");
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"reset.css"), "{imports:?}");
}

#[test]
fn at_import_with_url_function_creates_an_import_relation() {
    let parsed = parse("@import url(\"theme.css\");");
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"theme.css"), "{imports:?}");
}

#[test]
fn module_pseudo_symbol_owns_the_import_relation() {
    let parsed = parse("@import \"reset.css\";");
    let module = parsed
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Module)
        .unwrap();
    let import = parsed
        .relations
        .iter()
        .find(|r| r.kind == RelationKind::Imports)
        .unwrap();
    assert_eq!(import.from, module.id);
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = CssParser.parse(&SourceFile {
        relative_path: "broken.css".to_string(),
        contents: ".nav {{{ color".to_string(),
    });
    assert!(
        matches!(result, Err(mct_core::ParseError::Syntax { .. })),
        "{result:?}"
    );
}
