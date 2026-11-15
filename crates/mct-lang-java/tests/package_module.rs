//! A `package a.b;` clause is a `module` symbol named `a.b` that spans the
//! file, one per file (not deduplicated), as Kotlin's package header, Go's
//! package clause and C#'s namespace are. A file with no package clause
//! gets only its file module.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::SymbolKind;
use mct_lang_java::JavaParser;

fn modules(source: &str) -> Vec<(String, u32, Option<u32>)> {
    mct_corpus::parse_source(&JavaParser, "src/shop/Order.java", "Java", source)
        .symbols
        .into_iter()
        .filter(|s| s.kind == SymbolKind::Module)
        .map(|s| (s.name, s.location.line, s.location.end_line))
        .collect()
}

#[test]
fn a_package_clause_is_a_module_spanning_the_file() {
    let found = modules("package com.shop.orders;\n\nimport java.util.List;\n\nclass Order {}\n");
    assert!(
        found.contains(&("com.shop.orders".to_string(), 1, Some(5))),
        "{found:?}"
    );
    // Plus the file's own module.
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn a_file_without_a_package_has_only_its_file_module() {
    let found = modules("class Order {}\n");
    assert_eq!(found.len(), 1, "{found:?}");
}
