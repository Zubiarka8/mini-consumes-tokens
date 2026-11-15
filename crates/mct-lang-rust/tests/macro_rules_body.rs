//! A function named in the transcriber of a `macro_rules!` rule
//! (`helper($x) * 2`, `util::run($x)`) is used by the macro: a relation
//! from the macro, so it is not dead code. The matcher side and a
//! metavariable that shares a function's name (`$helper`) record nothing.

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_rust::RustParser;

const SOURCE: &str = r#"macro_rules! sq {
    ($x:expr) => { helper($x) * 2 };
}
macro_rules! pass {
    ($helper:ident) => { $helper(1) };
}
macro_rules! viamod {
    ($x:expr) => { util::run($x) };
}
fn helper(x: i32) -> i32 { x }
fn run() {}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&RustParser, "src/lib.rs", "Rust", SOURCE)
}

/// `(owner, kind, name)` of the relations whose owner is a macro.
fn from_macros(parsed: &ParsedFile) -> Vec<(String, RelationKind, String)> {
    let mut rows: Vec<_> = parsed
        .relations
        .iter()
        .filter_map(|r| {
            let owner = parsed.symbols.iter().find(|s| s.id == r.from)?;
            matches!(owner.name.as_str(), "sq" | "pass" | "viamod")
                .then(|| (owner.name.clone(), r.kind, r.to_name.clone()))
        })
        .collect();
    rows.sort_by(|a, b| (&a.0, &a.2).cmp(&(&b.0, &b.2)));
    rows
}

#[test]
fn a_function_in_a_macro_transcriber_is_used_by_the_macro() {
    let rows = from_macros(&parsed());
    assert!(
        rows.contains(&("sq".into(), RelationKind::References, "helper".into())),
        "{rows:?}"
    );
}

#[test]
fn a_metavariable_named_like_a_function_is_no_use() {
    let rows = from_macros(&parsed());
    assert!(!rows.iter().any(|r| r.0 == "pass"), "{rows:?}");
}

#[test]
fn a_path_call_in_a_transcriber_is_a_relation_of_the_macro() {
    let rows = from_macros(&parsed());
    assert!(
        rows.iter().any(|r| r.0 == "viamod" && r.2 == "run"),
        "{rows:?}"
    );
}
