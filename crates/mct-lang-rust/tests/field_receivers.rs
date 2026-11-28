//! A call through a typed field (`self.index.read_snapshot()`) resolves to
//! the field type's method, not to a same-named method of another type,
//! and to a trait's default method the field type inherits.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::Index;
use mct_lang_rust::RustParser;

const LIB_RS: &str = "pub struct Index;
impl Index {
    pub fn read_snapshot(&self) {}
}
pub struct Cache;
impl Cache {
    pub fn read_snapshot(&self) {}
}
pub trait Evict {
    fn evict(&self) {}
}
impl Evict for Index {}
pub struct Other;
impl Evict for Other {
    fn evict(&self) {}
}
pub struct Server {
    index: Index,
}
impl Server {
    pub fn handle(&self) {
        self.index.read_snapshot();
        self.index.evict();
    }
}
";

fn indexed() -> Index {
    let root = std::env::temp_dir().join(format!("mct-field-receivers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), LIB_RS).unwrap();
    mct_corpus::open_indexed(&root, Arc::new(RustParser), 1, "src/lib.rs")
}

/// `handle`'s calls as `name resolution target-line`.
fn calls_of_handle(index: &Index) -> Vec<String> {
    index
        .find_calls("handle")
        .unwrap()
        .into_iter()
        .map(|hit| {
            let line = hit
                .target_id
                .map(|id| index.symbols_by_ids(&[id]).unwrap()[0].line.to_string())
                .unwrap_or_else(|| hit.candidate_count.to_string() + " candidates");
            format!("{} {} {line}", hit.to_name, hit.resolution.as_str())
        })
        .collect()
}

#[test]
fn a_typed_field_picks_its_type_s_method_and_an_inherited_default() {
    let index = indexed();
    assert_eq!(
        calls_of_handle(&index),
        ["read_snapshot resolved 3", "evict resolved 10"]
    );
}

/// A test file may import its own package by name, so `use tempfile::TempDir`
/// there does not prove `TempDir` is another crate's: a field of that type
/// proves nothing, and `self.directory.path()` must not reach the
/// repository's own `TempDir`.
#[test]
fn a_field_typed_by_another_crate_in_a_test_file_proves_nothing() {
    let root = std::env::temp_dir().join(format!("mct-field-imported-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub struct TempDir;\nimpl TempDir {\n    pub fn path(&self) {}\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("tests/project.rs"),
        "use tempfile::TempDir;\n\
         struct Project {\n    directory: TempDir,\n}\n\
         impl Project {\n    fn handle(&self) {\n        self.directory.path();\n    }\n}\n",
    )
    .unwrap();
    let index = mct_corpus::open_indexed(
        &root,
        Arc::new(RustParser),
        2,
        "src/lib.rs, tests/project.rs",
    );
    let calls = calls_of_handle(&index);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(!calls[0].contains(" resolved "), "{calls:?}");
}
