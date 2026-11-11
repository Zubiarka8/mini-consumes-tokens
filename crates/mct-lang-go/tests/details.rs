//! Declaration details (ADR-034, issue #321) the Go parser proves from
//! syntax: the header as written (no doc comment or body), receiver and
//! parameters, results, generics, and visibility from the exported-name rule.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParameterKind, ParsedFile, SymbolDetail, Visibility};
use mct_lang_go::GoParser;

const SOURCE: &str = "package stack

// Stack holds items.
type Stack[T any] struct {
	items []T
}

type reader interface {
	Read(p []byte) (n int, err error)
}

type ID = string

// Push adds items.
func (s *Stack[T]) Push(first T,
	rest ...T) {
}

func merge[K comparable, V any](a, b map[K]V, _ func(int) bool) (map[K]V, error) {
	return nil, nil
}

func (Stack[T]) size() int { return 0 }
";

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&GoParser, "stack/stack.go", "Go", SOURCE)
}

fn detail(parsed: &ParsedFile, name: &str, line: u32) -> SymbolDetail {
    let symbol = parsed
        .symbols
        .iter()
        .find(|s| s.name == name && s.location.line == line)
        .unwrap_or_else(|| panic!("no {name} on line {line}"));
    parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == symbol.id)
        .cloned()
        .unwrap_or_else(|| panic!("no detail for {name}"))
        .normalized()
}

/// `name:type:kind` per parameter, `_` for no name.
fn params(detail: &SymbolDetail) -> Vec<String> {
    detail
        .parameters
        .iter()
        .map(|p| {
            format!(
                "{}:{}:{}",
                p.name.as_deref().unwrap_or("_"),
                p.type_text.as_ref().map_or("?", |t| t.text.as_str()),
                p.kind.as_str()
            )
        })
        .collect()
}

#[test]
fn a_method_header_spans_lines_and_records_its_receiver_and_variadic() {
    let parsed = parsed();
    let push = detail(&parsed, "Push", 15);
    assert_eq!(
        push.signature.as_deref(),
        Some("func (s *Stack[T]) Push(first T, rest ...T)")
    );
    assert_eq!(push.visibility, Some(Visibility::Public));
    assert!(push.parameters_known);
    assert_eq!(
        params(&push),
        [
            "s:*Stack[T]:receiver",
            "first:T:positional",
            "rest:T:variadic"
        ]
    );
    assert!(push.return_type.is_none());
}

#[test]
fn shared_types_unnamed_parameters_results_and_generics_are_kept() {
    let parsed = parsed();
    let merge = detail(&parsed, "merge", 19);
    assert_eq!(merge.visibility, Some(Visibility::Package));
    assert_eq!(merge.generics.as_deref(), Some("[K comparable, V any]"));
    assert_eq!(
        params(&merge),
        [
            "a:map[K]V:positional",
            "b:map[K]V:positional",
            "_:func(int) bool:positional"
        ]
    );
    assert_eq!(
        merge.return_type.map(|t| t.text).as_deref(),
        Some("(map[K]V, error)")
    );
    let size = detail(&parsed, "size", 23);
    assert_eq!(params(&size), ["_:Stack[T]:receiver"]);
    assert_eq!(size.return_type.map(|t| t.text).as_deref(), Some("int"));
}

#[test]
fn types_and_interface_methods_get_their_header() {
    let parsed = parsed();
    let stack = detail(&parsed, "Stack", 4);
    assert_eq!(stack.signature.as_deref(), Some("type Stack[T any] struct"));
    assert_eq!(stack.generics.as_deref(), Some("[T any]"));
    assert_eq!(stack.visibility, Some(Visibility::Public));
    assert!(!stack.parameters_known);
    let reader = detail(&parsed, "reader", 8);
    assert_eq!(reader.signature.as_deref(), Some("type reader interface"));
    assert_eq!(reader.visibility, Some(Visibility::Package));
    let read = detail(&parsed, "Read", 9);
    assert_eq!(
        read.signature.as_deref(),
        Some("Read(p []byte) (n int, err error)")
    );
    assert_eq!(params(&read), ["p:[]byte:positional"]);
    assert_eq!(
        read.return_type.map(|t| t.text).as_deref(),
        Some("(n int, err error)")
    );
    let id = detail(&parsed, "ID", 12);
    assert_eq!(id.signature.as_deref(), Some("type ID = string"));
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-go-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("stack")).unwrap();
    std::fs::write(root.join("stack/stack.go"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(GoParser));
    let push = index
        .symbol_details("stack/stack.go", 15, "Push")
        .unwrap()
        .unwrap();
    assert_eq!(push.visibility.as_deref(), Some("public"));
    assert_eq!(push.parameters.len(), 3);
    assert_eq!(push.parameters[0].kind, ParameterKind::Receiver.as_str());
    std::fs::remove_dir_all(root).unwrap();
}
