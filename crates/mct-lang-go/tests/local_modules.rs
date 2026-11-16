//! A module path without a dot (`module shop`, imports `shop/store`) names a
//! repository package, not the standard library: `store.Open()` and the
//! aliased `st.Open()` reach the declaration under `store/`, never a
//! same-named one in another directory. A path rooted at a standard
//! directory (`net/http`, `log`) stays the standard library's, and a path
//! whose root is a dotless module that is also a standard root is left
//! unproven.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::Resolution;
use mct_lang_go::GoParser;

const RUN: &str = r#"package main

import (
	"log"
	st "shop/store"
	"shop/cache"
)

func run() {
	st.Open()
	cache.Open()
	log.Println("x")
}
"#;

#[test]
fn a_dotless_module_import_reaches_its_directory() {
    let root = std::env::temp_dir().join(format!("mct-go-local-{}", std::process::id()));
    for dir in ["cmd", "store", "cache", "elsewhere"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("cmd/run.go"), RUN).unwrap();
    for dir in ["store", "cache", "elsewhere"] {
        std::fs::write(
            root.join(dir).join("open.go"),
            format!("package {dir}\n\nfunc Open() {{}}\n"),
        )
        .unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(GoParser));
    let mut hits: Vec<_> = index
        .find_references("Open")
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == "cmd/run.go" && h.kind == "calls")
        .collect();
    hits.sort_by_key(|h| h.line);
    assert_eq!(hits.len(), 2, "{hits:?}");
    for (hit, dir) in hits.iter().zip(["store", "cache"]) {
        assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
        assert_eq!(
            hit.candidates[0].relative_path,
            format!("{dir}/open.go"),
            "{hit:?}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
