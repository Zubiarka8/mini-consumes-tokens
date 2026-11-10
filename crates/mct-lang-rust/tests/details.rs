//! Declaration details (ADR-034, issue #321) the Rust parser proves from
//! syntax: the header as written (no attributes, doc comment or body),
//! parameters, return type, visibility, modifiers and generics.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{Modifier, ParameterKind, ParsedFile, Provenance, SymbolDetail, Visibility};
use mct_lang_rust::RustParser;

const SOURCE: &str = r#"
/// Runs it.
#[inline]
pub(crate) async unsafe fn run<T: Clone, const N: usize>(
    items: &[T; N],
    mut count: usize,
    (a, b): (u8, u8),
    _: impl Into<String>,
) -> Result<Vec<T>, String>
where
    T: Default,
{
    Ok(Vec::new())
}

fn quiet() {}

pub struct Stack<T> {
    items: Vec<T>,
}

impl<T> Stack<T> {
    pub const fn len(&self) -> usize {
        0
    }

    fn push(&mut self, item: T) {}
}

pub trait Shape {
    fn area(&self) -> f64;
}

impl Shape for Stack<u8> {
    fn area(&self) -> f64 {
        0.0
    }
}

pub type Pair = (u8, u8);
pub(super) const LIMIT: usize = 3;
extern "C" fn callback(code: i32) {}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&RustParser, "src/lib.rs", "Rust", SOURCE)
}

/// The detail of the symbol named `name` declared on `line`.
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
}

#[test]
fn a_function_header_is_recorded_without_attributes_doc_or_body() {
    let parsed = parsed();
    let run = detail(&parsed, "run", 4).normalized();
    assert_eq!(
        run.signature.as_deref(),
        Some(
            "pub(crate) async unsafe fn run<T: Clone, const N: usize>( items: &[T; N], \
             mut count: usize, (a, b): (u8, u8), _: impl Into<String>, ) \
             -> Result<Vec<T>, String> where T: Default,"
        )
    );
    assert_eq!(run.visibility, Some(Visibility::Crate));
    assert_eq!(run.modifiers, [Modifier::Async, Modifier::Unsafe]);
    assert_eq!(run.generics.as_deref(), Some("<T: Clone, const N: usize>"));
    let ret = run.return_type.unwrap();
    assert_eq!(
        (ret.text.as_str(), ret.provenance),
        ("Result<Vec<T>, String>", Provenance::Syntax)
    );
    assert!(run.parameters_known);
    let params: Vec<_> = run
        .parameters
        .iter()
        .map(|p| {
            (
                p.name.clone(),
                p.type_text.as_ref().map(|t| t.text.clone()),
                p.kind,
            )
        })
        .collect();
    assert_eq!(
        params,
        [
            (
                Some("items".into()),
                Some("&[T; N]".into()),
                ParameterKind::Positional
            ),
            (
                Some("count".into()),
                Some("usize".into()),
                ParameterKind::Positional
            ),
            (None, Some("(u8, u8)".into()), ParameterKind::Positional),
            (
                None,
                Some("impl Into<String>".into()),
                ParameterKind::Positional
            ),
        ]
    );
}

#[test]
fn a_free_function_without_a_visibility_keyword_is_private_and_has_no_parameters() {
    let parsed = parsed();
    let quiet = detail(&parsed, "quiet", 16);
    assert_eq!(quiet.signature.as_deref(), Some("fn quiet()"));
    assert_eq!(quiet.visibility, Some(Visibility::Private));
    assert!(quiet.parameters_known);
    assert!(quiet.parameters.is_empty());
    assert!(quiet.return_type.is_none());
}

#[test]
fn a_method_records_its_receiver_and_a_trait_method_has_unknown_visibility() {
    let parsed = parsed();
    let len = detail(&parsed, "len", 23);
    assert_eq!(len.visibility, Some(Visibility::Public));
    assert_eq!(len.modifiers, [Modifier::Const]);
    assert_eq!(len.parameters[0].name.as_deref(), Some("self"));
    assert_eq!(len.parameters[0].kind, ParameterKind::Receiver);
    assert_eq!(
        len.parameters[0]
            .type_text
            .as_ref()
            .map(|t| t.text.as_str()),
        Some("&Self")
    );
    // An inherent method without `pub` is private to the module.
    assert_eq!(
        detail(&parsed, "push", 27).visibility,
        Some(Visibility::Private)
    );
    // A trait's method, and its implementation, are as visible as the trait:
    // the syntax of the item itself does not say.
    let declared = detail(&parsed, "area", 31);
    assert_eq!(declared.signature.as_deref(), Some("fn area(&self) -> f64"));
    assert_eq!(declared.visibility, None);
    assert_eq!(detail(&parsed, "area", 35).visibility, None);
}

#[test]
fn types_aliases_constants_and_extern_functions_get_their_header() {
    let parsed = parsed();
    let stack = detail(&parsed, "Stack", 18);
    assert_eq!(stack.signature.as_deref(), Some("pub struct Stack<T>"));
    assert_eq!(stack.generics.as_deref(), Some("<T>"));
    assert!(!stack.parameters_known);
    let shape = detail(&parsed, "Shape", 30);
    assert_eq!(shape.signature.as_deref(), Some("pub trait Shape"));
    let pair = detail(&parsed, "Pair", 40);
    assert_eq!(pair.signature.as_deref(), Some("pub type Pair = (u8, u8)"));
    let limit = detail(&parsed, "LIMIT", 41);
    assert_eq!(
        limit.signature.as_deref(),
        Some("pub(super) const LIMIT: usize")
    );
    assert_eq!(limit.visibility, Some(Visibility::Restricted));
    let callback = detail(&parsed, "callback", 42);
    assert_eq!(callback.modifiers, [Modifier::Extern]);
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-rust-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));
    let run = index
        .symbol_details("src/lib.rs", 4, "run")
        .unwrap()
        .unwrap();
    assert_eq!(run.visibility.as_deref(), Some("crate"));
    assert_eq!(run.modifiers, ["async", "unsafe"]);
    assert_eq!(run.parameters.len(), 4);
    std::fs::remove_dir_all(root).unwrap();
}
