//! `u64::from(x)`, `u64::try_from(x)`, `u64::into(x)`, `u64::try_into(x)`
//! on a primitive type: core's blanket impls (`impl<T> From<T> for T`,
//! `impl<T, U: Into<T>> TryFrom<U> for T`, and the `Into`/`TryInto` they
//! give) always implement the trait for that type too, and a primitive has
//! no inherent `from`, so a repository's `impl From<Id> for u64` is one
//! candidate among several: ambiguous, never resolved. A module named like
//! the primitive (`mod u64`) is a path like any other.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_lang_rust::RustParser;

const ID: &str = "\
pub struct Id(pub u64);

impl From<Id> for u64 {
    fn from(id: Id) -> u64 {
        id.0
    }
}

pub struct Count(pub u8);

impl Count {
    pub fn from(n: u8) -> Count {
        Count(n)
    }
}
";

const USE: &str = "\
use crate::id::{Count, Id};

pub fn widen(n: u32, id: Id) -> (u64, u64, Count) {
    (u64::from(n), u64::from(id), Count::from(1))
}

pub fn listed(n: u32) -> Vec<u64> {
    vec![u64::from(n)]
}
";

const SHADOW: &str = "\
mod u64 {
    pub fn from(n: u32) -> u32 {
        n
    }
}

pub fn call(n: u32) -> u32 {
    u64::from(n)
}
";

#[test]
fn a_conversion_on_a_primitive_is_never_resolved_to_one_impl() {
    let root = std::env::temp_dir().join(format!("mct-rs-prim-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/id.rs"), ID).unwrap();
    std::fs::write(root.join("src/use_id.rs"), USE).unwrap();
    std::fs::write(root.join("src/shadow.rs"), SHADOW).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));
    let mut hits: Vec<String> = index
        .find_references("from")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind != "overrides")
        .map(|h| {
            format!(
                "{}:{}:{} {} {:?}",
                h.relative_path, h.line, h.column, h.kind, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "src/shadow.rs:8:10 calls Resolved",
            "src/use_id.rs:4:11 calls Ambiguous",
            "src/use_id.rs:4:25 calls Ambiguous",
            "src/use_id.rs:4:42 calls Resolved",
            "src/use_id.rs:8:15 references Ambiguous",
        ]
    );
}
