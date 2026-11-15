//! An item of an inline `mod name { … }` has that module as its parent, the
//! way a C++ namespace member does, so a path through the module
//! (`inner::helper()`) resolves to it with the module evidence it carries.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{LanguageParser, SourceFile};
use mct_lang_rust::RustParser;

const LIB: &str = r#"mod inner {
    pub fn helper() -> u8 { 1 }
    pub struct Thing;
    pub mod deeper {
        pub fn leaf() -> u8 { 2 }
    }
}
mod other;

pub fn entry() -> u8 {
    let _t: inner::Thing = inner::Thing;
    inner::helper() + inner::deeper::leaf() + other::helper() + missing::helper()
}
"#;

const OTHER: &str = "pub fn helper() -> u8 { 3 }\n";

#[test]
fn an_inline_module_item_has_the_module_as_its_parent() {
    let parsed = RustParser
        .parse(&SourceFile {
            relative_path: "src/lib.rs".to_string(),
            contents: LIB.to_string(),
        })
        .unwrap();
    let mut parents: Vec<String> = parsed
        .symbols
        .iter()
        .map(|s| format!("{}:{}", s.name, s.parent.as_deref().unwrap_or("-")))
        .collect();
    parents.sort();
    assert_eq!(
        parents,
        [
            "Thing:inner",
            "deeper:inner",
            "_t:entry",
            "entry:-",
            "helper:inner",
            "inner:-",
            "leaf:deeper",
            "lib:-",
            "other:-",
        ]
    );
}

#[test]
fn a_path_through_an_inline_module_resolves_to_its_item() {
    let root = std::env::temp_dir().join(format!("mct-rs-inline-mod-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), LIB).unwrap();
    std::fs::write(root.join("src/other.rs"), OTHER).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));
    let mut hits: Vec<String> = index
        .find_calls("entry")
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.to_name, h.column, h.resolution))
        .collect();
    hits.sort();
    // A type path through the module (`inner::Thing`) reaches the struct.
    let things: Vec<String> = index
        .find_references("Thing")
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.line, h.column, h.resolution))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert!(!things.is_empty());
    assert!(things.iter().all(|t| t.ends_with("Resolved")), "{things:?}");
    // Each path through `inner` reaches its own item (the only one whose
    // parent is the module); `other::helper` the file module's; and
    // `missing::helper`, through no module of the project, nothing.
    assert_eq!(
        hits,
        [
            "helper:12 Resolved",
            "helper:54 Resolved",
            "helper:74 Unresolved",
            "leaf:38 Resolved",
        ]
    );
}
