//! A type named in a turbofish (`size_of::<Turbo>()`, `o.is::<Marker>()`,
//! `Vec::<Gen>::new()`) is a `references` relation from the function that
//! writes it, so a type used only that way is not dead code.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_rust::RustParser;

const SOURCE: &str = r#"struct Turbo;
struct Marker;
struct Gen;
struct Item;

fn size() -> usize { std::mem::size_of::<Turbo>() }
fn check(o: &dyn std::any::Any) -> bool { o.is::<Marker>() }
fn make() { let _ = Vec::<Gen>::new(); }
fn parse() { let _ = "1".parse::<Item>(); }
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&RustParser, "src/lib.rs", "Rust", SOURCE)
}

fn references(file: &ParsedFile) -> Vec<(String, String)> {
    file.relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| {
            let from = file.symbols.iter().find(|s| s.id == r.from).unwrap();
            (from.name.clone(), r.to_name.clone())
        })
        .collect()
}

#[test]
fn a_type_in_a_turbofish_is_referenced_by_the_function_that_writes_it() {
    let refs = references(&parsed());
    for expected in [
        ("size", "Turbo"),
        ("check", "Marker"),
        ("make", "Gen"),
        ("parse", "Item"),
    ] {
        assert!(
            refs.iter()
                .any(|(from, to)| (from.as_str(), to.as_str()) == expected),
            "{expected:?} missing from {refs:?}"
        );
    }
}
