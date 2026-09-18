// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-css/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_css::CssParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    CssParser
        .parse(&SourceFile { relative_path: "style.css".to_string(), contents: src.to_string() })
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
    assert!(parsed.symbols.iter().any(|s| s.name == ".a" && s.kind == SymbolKind::Rule));
    assert!(parsed.symbols.iter().any(|s| s.name == ".b" && s.kind == SymbolKind::Rule));
}

#[test]
fn compound_selector_is_not_indexed() {
    let parsed = parse("div.card { color: red; }");
    assert!(
        parsed.symbols.iter().all(|s| s.kind != SymbolKind::Rule),
        "div.card is compound (tag + class), out of scope for v1: {:?}",
        parsed.symbols
    );
}

#[test]
fn descendant_combinator_is_not_indexed() {
    let parsed = parse(".card .title { color: red; }");
    assert!(
        parsed.symbols.iter().all(|s| s.kind != SymbolKind::Rule),
        "a descendant combinator is out of scope for v1: {:?}",
        parsed.symbols
    );
}

#[test]
fn bare_tag_selector_is_not_indexed() {
    let parsed = parse("div { color: red; }");
    assert!(parsed.symbols.iter().all(|s| s.kind != SymbolKind::Rule));
}

#[test]
fn rules_nested_in_a_media_query_are_still_indexed() {
    let parsed = parse("@media (min-width: 600px) { .nav { display: flex; } }");
    assert!(parsed.symbols.iter().any(|s| s.name == ".nav" && s.kind == SymbolKind::Rule));
}

#[test]
fn at_import_with_a_plain_string_creates_an_import_relation() {
    let parsed = parse("@import \"reset.css\";");
    let imports: Vec<_> =
        parsed.relations.iter().filter(|r| r.kind == RelationKind::Imports).map(|r| r.to_name.as_str()).collect();
    assert!(imports.contains(&"reset.css"), "{imports:?}");
}

#[test]
fn at_import_with_url_function_creates_an_import_relation() {
    let parsed = parse("@import url(\"theme.css\");");
    let imports: Vec<_> =
        parsed.relations.iter().filter(|r| r.kind == RelationKind::Imports).map(|r| r.to_name.as_str()).collect();
    assert!(imports.contains(&"theme.css"), "{imports:?}");
}

#[test]
fn module_pseudo_symbol_owns_the_import_relation() {
    let parsed = parse("@import \"reset.css\";");
    let module = parsed.symbols.iter().find(|s| s.kind == SymbolKind::Module).unwrap();
    let import = parsed.relations.iter().find(|r| r.kind == RelationKind::Imports).unwrap();
    assert_eq!(import.from, module.id);
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = CssParser.parse(&SourceFile {
        relative_path: "broken.css".to_string(),
        contents: ".nav {{{ color".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })), "{result:?}");
}
