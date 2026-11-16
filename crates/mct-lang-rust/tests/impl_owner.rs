//! `impl Tr for T` is `T implements Tr`: the relation comes from `T` when
//! this file declares one struct, union or enum `T` (before or after the
//! `impl`, generic arguments and path ignored). A `T` declared elsewhere,
//! or twice here, leaves the file module as the owner: one file can't
//! prove which `T` it is.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_rust::RustParser;

const SOURCE: &str = r#"impl Display for Circle<f64> {
    fn fmt(&self) {}
}
pub struct Circle<T>(T);
pub enum Shape { A }
impl crate::shapes::Area for self::Shape {}
impl From<u8> for Remote {}
mod a { pub struct Twin; }
mod b { pub struct Twin; }
impl Clone for a::Twin {}
"#;

#[test]
fn an_impl_is_owned_by_the_type_this_file_declares() {
    let parsed = mct_corpus::parse_source(&RustParser, "src/shapes.rs", "Rust", SOURCE);
    let owner = |id| {
        let s = parsed.symbols.iter().find(|s| s.id == id).unwrap();
        format!("{} {:?}", s.name, s.kind)
    };
    let implements: Vec<(String, &str)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Implements)
        .map(|r| (owner(r.from), r.to_name.as_str()))
        .collect();
    assert_eq!(
        implements,
        [
            (format!("Circle {:?}", SymbolKind::Struct), "Display"),
            (format!("Shape {:?}", SymbolKind::Enum), "Area"),
            (format!("shapes {:?}", SymbolKind::Module), "From"),
            (format!("shapes {:?}", SymbolKind::Module), "Clone"),
        ]
    );
}
