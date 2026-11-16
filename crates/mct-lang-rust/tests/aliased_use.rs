//! D6 for Rust: `use bbb_lib::render as draw;` makes `draw()` a call to
//! `render` of `bbb_lib`, so it reaches that crate's item and never
//! `aaa_lib`'s `render`. The same import without an alias keeps resolving,
//! a `crate::` path proves no crate but still names the item, and an alias a
//! local binding shadows keeps its own name.
//!
//! The project is written to a temporary directory, so its clashing names
//! never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::Resolution;
use mct_lang_rust::RustParser;

const APP: &str = r#"use aaa_lib::render;
use bbb_lib::render as draw;
use crate::util::helper as assist;

pub fn run() {
    render();
    draw();
    assist();
}

pub fn shadowed() {
    let draw = || ();
    draw();
}
"#;

const LIB: &str = "pub fn render() {}\n";
const UTIL: &str = "pub fn helper() {}\n";

#[test]
fn an_aliased_use_names_the_original_item_in_its_crate() {
    let root = std::env::temp_dir().join(format!("mct-rs-aliased-use-{}", std::process::id()));
    for dir in ["src", "crates/aaa-lib/src", "crates/bbb-lib/src"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("src/app.rs"), APP).unwrap();
    std::fs::write(root.join("src/util.rs"), UTIL).unwrap();
    std::fs::write(root.join("crates/aaa-lib/src/lib.rs"), LIB).unwrap();
    std::fs::write(root.join("crates/bbb-lib/src/lib.rs"), LIB).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));

    let render: Vec<_> = index
        .find_references("render")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls" && h.relative_path == "src/app.rs")
        .collect();
    let by_line = |line: u32| render.iter().find(|h| h.line == line).unwrap();
    let plain = by_line(6);
    assert_eq!(plain.resolution, Resolution::Resolved, "{plain:?}");
    assert_eq!(
        plain.candidates[0].relative_path, "crates/aaa-lib/src/lib.rs",
        "{plain:?}"
    );
    let aliased = by_line(7);
    assert_eq!(aliased.resolution, Resolution::Resolved, "{aliased:?}");
    assert_eq!(
        aliased.candidates[0].relative_path, "crates/bbb-lib/src/lib.rs",
        "{aliased:?}"
    );

    // A `crate::` alias names the original item, with no crate evidence.
    let helper: Vec<_> = index
        .find_references("helper")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls" && h.relative_path == "src/app.rs")
        .collect();
    assert_eq!(helper.len(), 1, "{helper:?}");

    // A local `draw` shadows the alias: that call keeps its name.
    let local: Vec<_> = index
        .find_references("draw")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "calls" && h.relative_path == "src/app.rs")
        .collect();
    assert_eq!(local.len(), 1, "{local:?}");
    assert_eq!(local[0].line, 13, "{local:?}");
    std::fs::remove_dir_all(root).unwrap();
}
