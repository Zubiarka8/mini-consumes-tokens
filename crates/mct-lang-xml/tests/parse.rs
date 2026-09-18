// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-xml/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, SourceFile, SymbolKind};
use mct_lang_xml::XmlParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    XmlParser
        .parse(&SourceFile { relative_path: "config.xml".to_string(), contents: src.to_string() })
        .expect("valid XML source should parse")
}

#[test]
fn extracts_element_with_id_attribute() {
    let parsed = parse("<root><server id=\"api\"></server></root>");
    let el = parsed.symbols.iter().find(|s| s.name == "api").unwrap();
    assert_eq!(el.kind, SymbolKind::Element);
}

#[test]
fn extracts_element_with_lowercase_name_attribute() {
    let parsed = parse("<root><endpoint name=\"health\"></endpoint></root>");
    let el = parsed.symbols.iter().find(|s| s.name == "health").unwrap();
    assert_eq!(el.kind, SymbolKind::Element);
}

#[test]
fn extracts_element_with_capitalized_name_attribute() {
    let parsed = parse("<root><Task Name=\"Build\"></Task></root>");
    let el = parsed.symbols.iter().find(|s| s.name == "Build").unwrap();
    assert_eq!(el.kind, SymbolKind::Element);
}

#[test]
fn id_takes_priority_over_name_when_both_present() {
    let parsed = parse("<item id=\"by-id\" name=\"by-name\"></item>");
    assert!(parsed.symbols.iter().any(|s| s.name == "by-id"));
    assert!(!parsed.symbols.iter().any(|s| s.name == "by-name"));
}

#[test]
fn element_without_any_naming_attribute_is_not_indexed() {
    let parsed = parse("<root><item>plain</item></root>");
    assert!(parsed.symbols.iter().all(|s| s.kind != SymbolKind::Element));
}

#[test]
fn nested_named_elements_track_parent() {
    let parsed = parse("<server id=\"api\"><endpoint name=\"health\"></endpoint></server>");
    let endpoint = parsed.symbols.iter().find(|s| s.name == "health").unwrap();
    assert_eq!(endpoint.parent.as_deref(), Some("api"));
}

#[test]
fn no_relations_are_ever_emitted() {
    let parsed = parse("<root><server id=\"api\" name=\"ignored-when-id-present\"></server></root>");
    assert!(parsed.relations.is_empty(), "generic XML is structural-only by design: {:?}", parsed.relations);
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = XmlParser.parse(&SourceFile {
        relative_path: "broken.xml".to_string(),
        contents: "<root><unclosed>".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })), "{result:?}");
}
