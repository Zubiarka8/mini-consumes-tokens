//! D8 (issue #349): each `fn` of an `impl Trait for T` overrides the
//! trait's method of that name — an `Overrides` relation qualified by the
//! trait, never by a shared name alone. An inherent `impl T` overrides
//! nothing, and a same-named method of an unrelated trait is not a target.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_rust::RustParser;

const SOURCE: &str = r#"
pub trait Shape {
    fn area(&self) -> f64;
}
pub trait Named {
    fn area(&self) -> f64;
}
pub struct Square(f64);
impl Shape for Square {
    fn area(&self) -> f64 { self.0 * self.0 }
}
impl Square {
    pub fn side(&self) -> f64 { self.0 }
}
impl std::fmt::Display for Square {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { Ok(()) }
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&RustParser, "src/shape.rs", "Rust", SOURCE)
}

/// `from -> to (qualifier)` for every `Overrides` relation.
fn overrides(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Overrides)
        .map(|(i, r)| {
            let from = parsed.symbols.iter().find(|s| s.id == r.from).unwrap();
            let qualifier = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.qualifier.clone())
                .unwrap_or_default();
            format!(
                "{}.{} -> {} ({qualifier})",
                from.parent.as_deref().unwrap_or(""),
                from.name,
                r.to_name
            )
        })
        .collect()
}

#[test]
fn trait_impl_methods_override_the_trait_named_in_the_impl() {
    assert_eq!(
        overrides(&parsed()),
        ["Square.area -> area (Shape)", "Square.fmt -> fmt (Display)"]
    );
}

#[test]
fn the_index_resolves_to_the_named_trait_and_keeps_std_external() {
    let root = std::env::temp_dir().join(format!("mct-rust-overrides-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/shape.rs"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));
    let mut hits: Vec<String> = ["area", "fmt"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "overrides")
        .map(|h| {
            let target = h
                .target_id
                .and_then(|id| index.symbols_by_ids(&[id]).unwrap().pop())
                .map(|s| format!("{}:{}", s.parent.unwrap_or_default(), s.line))
                .unwrap_or_default();
            format!("{} {:?} {target}", h.to_name, h.resolution)
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["area Resolved Shape:3", "fmt External "]);
}
