//! D9: a call whose receiver type is bound by a single-type JDK import
//! (`import java.util.List;`: `List.of()`, `items.add()` on a `List
//! items`) is external, never a method of a repository type named `List`.
//! A single-type import shadows the package's own types (JLS §6.4.1).
//! Not proven: a name the file also declares as a type or type parameter,
//! a static or on-demand import, and `java.lang` names without an import.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_java::JavaParser;

const CART: &str = r#"package shop;

import java.util.List;
import java.util.Map;
import java.util.*;
import static java.util.Objects.requireNonNull;

public class Cart {
    private List<String> items;
    private Map<String, Integer> counts;

    public int add(String item) {
        items = List.of();
        counts = Map.of();
        items.add(item);
        Set<String> seen = new HashSet<>();
        seen.add(item);
        requireNonNull(item);
        return Math.max(counts.size(), 0);
    }
}
"#;

const SHADOWED: &str = r#"package shop;

import java.util.Map;

public class Index {
    static <Map> void generic(Map m) {
        m.size();
    }
}
"#;

const LIST: &str = r#"package shop;

public class List {
    public static List of() { return new List(); }
    public boolean add(String s) { return true; }
}
"#;

fn parsed(path: &str, source: &str) -> ParsedFile {
    mct_corpus::parse_source(&JavaParser, path, "Java", source)
}

/// Every call as `name` plus ` external` when the parser proved it so.
fn calls(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .map(|(i, r)| {
            let external = parsed
                .relation_targets
                .iter()
                .any(|t| t.relation == i && t.external);
            format!("{}{}", r.to_name, if external { " external" } else { "" })
        })
        .collect()
}

#[test]
fn a_call_on_a_single_type_jdk_import_is_external() {
    assert_eq!(
        calls(&parsed("shop/Cart.java", CART)),
        [
            "of external",
            "of external",
            "add external",
            "HashSet",
            "add",
            "requireNonNull",
            "max",
            "size external",
        ]
    );
}

#[test]
fn a_type_parameter_of_the_imported_name_is_not_proven() {
    assert_eq!(calls(&parsed("shop/Index.java", SHADOWED)), ["size"]);
}

#[test]
fn a_same_package_type_of_that_name_is_never_the_target() {
    let root = std::env::temp_dir().join(format!("mct-java-jdk-{}", std::process::id()));
    std::fs::create_dir_all(root.join("shop")).unwrap();
    std::fs::write(root.join("shop/Cart.java"), CART).unwrap();
    std::fs::write(root.join("shop/List.java"), LIST).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JavaParser));
    let mut hits: Vec<String> = ["of", "add"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| {
            format!(
                "{} {}:{} {:?}",
                h.to_name, h.relative_path, h.line, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "add shop/Cart.java:15 External",
            "add shop/Cart.java:17 Unresolved",
            "of shop/Cart.java:13 External",
            "of shop/Cart.java:14 External",
        ]
    );
}
