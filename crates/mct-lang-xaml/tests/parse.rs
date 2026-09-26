// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-xaml/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_xaml::XamlParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    XamlParser
        .parse(&SourceFile {
            relative_path: "MainWindow.xaml".to_string(),
            contents: src.to_string(),
        })
        .expect("valid XAML source should parse")
}

#[test]
fn extracts_element_with_x_name() {
    let parsed = parse("<Window><Button x:Name=\"SaveBtn\"/></Window>");
    let el = parsed.symbols.iter().find(|s| s.name == "SaveBtn").unwrap();
    assert_eq!(el.kind, SymbolKind::Element);
}

#[test]
fn extracts_element_with_plain_name() {
    let parsed = parse("<Window><Button Name=\"SaveBtn\"/></Window>");
    let el = parsed.symbols.iter().find(|s| s.name == "SaveBtn").unwrap();
    assert_eq!(el.kind, SymbolKind::Element);
}

#[test]
fn x_name_takes_priority_over_plain_name() {
    let parsed = parse("<Button x:Name=\"ByXName\" Name=\"ByName\"/>");
    assert!(parsed.symbols.iter().any(|s| s.name == "ByXName"));
    assert!(!parsed.symbols.iter().any(|s| s.name == "ByName"));
}

#[test]
fn click_attribute_creates_a_reference_to_the_handler_name() {
    let parsed = parse("<Button x:Name=\"SaveBtn\" Click=\"SaveBtn_Click\"/>");
    let btn = parsed.symbols.iter().find(|s| s.name == "SaveBtn").unwrap();
    let refs: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.from == btn.id && r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(refs.contains(&"SaveBtn_Click"), "{refs:?}");
}

#[test]
fn event_attribute_without_a_name_still_attaches_to_the_module_root() {
    let parsed = parse("<Window Loaded=\"Window_Loaded\"></Window>");
    let module = parsed
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Module)
        .unwrap();
    let refs: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.from == module.id && r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(refs.contains(&"Window_Loaded"), "{refs:?}");
}

#[test]
fn binding_expression_is_not_treated_as_a_handler_name() {
    let parsed = parse("<Button Click=\"{Binding SaveCommand}\"/>");
    assert!(parsed
        .relations
        .iter()
        .all(|r| r.to_name != "{Binding SaveCommand}"));
    assert!(parsed.relations.is_empty(), "{:?}", parsed.relations);
}

#[test]
fn non_event_attribute_is_never_a_reference() {
    let parsed = parse("<Button Width=\"100\" Background=\"Red\"/>");
    assert!(parsed.relations.is_empty(), "{:?}", parsed.relations);
}

#[test]
fn nested_named_elements_track_parent() {
    let parsed = parse("<Window x:Name=\"Main\"><Button x:Name=\"SaveBtn\"/></Window>");
    let btn = parsed.symbols.iter().find(|s| s.name == "SaveBtn").unwrap();
    assert_eq!(btn.parent.as_deref(), Some("Main"));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = XamlParser.parse(&SourceFile {
        relative_path: "Broken.xaml".to_string(),
        contents: "<Window><Unclosed>".to_string(),
    });
    assert!(
        matches!(result, Err(mct_core::ParseError::Syntax { .. })),
        "{result:?}"
    );
}
