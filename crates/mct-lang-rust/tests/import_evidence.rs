//! D6 for Rust: a bare name a `use` brings in from another crate
//! (`use serde::Serialize`) names that crate's item, so a same-named item of
//! this repository is never its target. A crate of this repository is found
//! by its directory (`my_lib` in `crates/my-lib/`). A `crate::`, `self::` or
//! `super::` path adds no evidence (the item may sit in an inline `mod`), and
//! neither does an alias, a name imported from two crates, a path from a
//! module of this file, or any path in a file that may import its own
//! package by name (a test, example, bench, binary or build script).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_lang_rust::RustParser;

const APP: &str = r#"use serde::{Deserialize, Serialize};
use other_crate::helper;
use my_lib::Widget;
use crate::model::Thing;
use elsewhere::Gadget as Tool;

#[derive(Debug, Serialize, Deserialize)]
pub struct Order {
    widget: Widget,
    thing: Thing,
}

pub fn run(value: Serialize, tool: Tool) {
    helper();
    Widget::build();
}
"#;

const MODEL: &str = r#"pub struct Serialize;
pub struct Thing;
pub struct Tool;

pub fn helper() {}
"#;

const MY_LIB: &str = r#"pub struct Widget;

impl Widget {
    pub fn build() {}
}
"#;

#[test]
fn an_item_imported_from_another_crate_is_never_this_repositorys() {
    let root = std::env::temp_dir().join(format!("mct-rs-import-crate-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("crates/my-lib/src")).unwrap();
    std::fs::write(root.join("src/app.rs"), APP).unwrap();
    std::fs::write(root.join("src/model.rs"), MODEL).unwrap();
    std::fs::write(root.join("crates/my-lib/src/lib.rs"), MY_LIB).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));
    let mut hits: Vec<String> = ["Serialize", "helper", "Widget", "Thing", "Tool"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.relative_path == "src/app.rs" && h.kind != "imports")
        .map(|h| format!("{} {}:{} {:?}", h.kind, h.to_name, h.line, h.resolution))
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "calls helper:14 Unresolved",
            "references Serialize:13 Unresolved",
            "references Serialize:7 Unresolved",
            // `crate::model::Thing`: no crate evidence, found by name.
            "references Thing:10 Resolved",
            // An alias proves nothing; the real name is `Gadget`.
            "references Tool:13 Resolved",
            // `my_lib` is the crate in `crates/my-lib/`.
            "references Widget:15 Resolved",
            "references Widget:9 Resolved",
        ]
    );
}

#[test]
fn a_use_path_from_a_module_of_this_file_is_not_a_crate() {
    let root = std::env::temp_dir().join(format!("mct-rs-import-mod-{}", std::process::id()));
    std::fs::create_dir_all(root.join("examples")).unwrap();
    std::fs::create_dir_all(root.join("support")).unwrap();
    std::fs::write(
        root.join("examples/bench.rs"),
        "#[path = \"../support/token_bench.rs\"]\nmod bench;\nuse bench::Query;\n\nfn main() {\n    let q: Query = Query;\n}\n",
    )
    .unwrap();
    std::fs::write(root.join("support/token_bench.rs"), "pub struct Query;\n").unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));
    let mut hits: Vec<String> = index
        .find_references("Query")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind != "imports")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["examples/bench.rs:6 Resolved"]);
}

#[test]
fn a_library_module_import_is_not_mistaken_for_a_crate() {
    let root = std::env::temp_dir().join(format!("mct-rs-import-lib-mod-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("support")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "#[path = \"../support/query.rs\"]\nmod bench;\nuse bench::Query;\n\npub fn run(q: Query) {}\n",
    )
    .unwrap();
    std::fs::write(root.join("support/query.rs"), "pub struct Query;\n").unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));
    let hits: Vec<_> = index
        .find_references("Query")
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == "src/lib.rs" && h.kind != "imports")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["src/lib.rs:5 Resolved"]);
}

#[test]
fn an_integration_test_may_import_its_own_package_by_name() {
    // A package at the repository root: its crate name, `app`, is in no path.
    let root = std::env::temp_dir().join(format!("mct-rs-import-own-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub mod billing;\n\npub struct Widget;\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/billing.rs"),
        "pub fn create_invoice() -> u64 {\n    1\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("tests/it.rs"),
        "use app::billing::create_invoice;\nuse app::Widget;\n\n#[test]\nfn invoices() {\n    let _w: Widget = Widget;\n    create_invoice();\n}\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));
    let mut hits: Vec<String> = ["Widget", "create_invoice"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.relative_path == "tests/it.rs" && h.kind != "imports")
        .map(|h| format!("{} {}:{} {:?}", h.kind, h.to_name, h.line, h.resolution))
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            // `app` is this package, named in no path: no evidence in a test.
            "calls create_invoice:7 Resolved",
            "references Widget:6 Resolved",
        ]
    );
}
