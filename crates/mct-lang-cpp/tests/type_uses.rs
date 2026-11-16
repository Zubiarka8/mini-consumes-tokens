//! A type named in a declaration (`Point p;`, a field, a parameter, a
//! prototype's return type) is a `references` relation when this file
//! declares that type, pinned to it, and an external one when it is written
//! through `std::`. A template parameter (`T`) names no type, and a
//! "parameter" of a prototype-like local (`std::lock_guard g(m_);`) is a
//! value. A type declared only in another file gives no relation: nothing
//! limits a by-name match to types, so it could reach a method of the same
//! name.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_cpp::CppParser;

const SHOP: &str = r#"#include <mutex>
#include <string>
#include "widget.h"
struct Point { int x; };
using Count = int;
template <typename T> struct Box { T value; };
class Shop {
  Point origin;
  std::mutex m_;
  Count total() const;
};
Point make(Point p, Count n);
Point make(Point p, Count n) {
  std::lock_guard<std::mutex> g(m_);
  Point local;
  std::string s;
  Widget w;
  auto a = p;
  return local;
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CppParser, "src/shop.cpp", "C++", SHOP)
}

#[test]
fn a_declared_type_of_this_file_or_of_std_is_referenced() {
    let parsed = parsed();
    let name_of = |id| {
        parsed
            .symbols
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .name
            .clone()
    };
    let mut references: Vec<(String, &str, u32, &str)> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::References)
        .map(|(i, r)| {
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            let evidence = match target {
                Some(t) if t.external => "external",
                Some(t) if t.path.as_deref() == Some("src/shop.cpp") && t.kind.is_some() => {
                    "this file"
                }
                _ => "by name",
            };
            (
                name_of(r.from),
                r.to_name.as_str(),
                r.location.line,
                evidence,
            )
        })
        .collect();
    references.sort_by_key(|r| (r.2, r.1.to_string(), r.0.clone()));
    let shop = "shop".to_string();
    let make = "make".to_string();
    assert_eq!(
        references,
        [
            (shop.clone(), "Point", 8, "this file"),
            (shop.clone(), "mutex", 9, "external"),
            (shop.clone(), "Count", 10, "this file"),
            (make.clone(), "Count", 12, "this file"),
            (make.clone(), "Point", 12, "this file"),
            (shop.clone(), "Point", 12, "this file"),
            (make.clone(), "Count", 13, "this file"),
            (make.clone(), "Point", 13, "this file"),
            (shop.clone(), "Point", 13, "this file"),
            (make.clone(), "lock_guard", 14, "external"),
            (make.clone(), "Point", 15, "this file"),
            (make.clone(), "string", 16, "external"),
        ]
    );
}

#[test]
fn a_type_of_another_file_is_not_guessed_by_name() {
    let root = std::env::temp_dir().join(format!("mct-cpp-type-uses-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/widget.h"),
        "class Widget {\n public:\n  Widget();\n  int path() const;\n};\n",
    )
    .unwrap();
    std::fs::write(root.join("src/shop.cpp"), SHOP).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CppParser));
    let mut hits: Vec<String> = ["Widget", "Point"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "references")
        .map(|h| {
            format!(
                "{}:{} {} {:?}",
                h.relative_path, h.line, h.to_name, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "src/shop.cpp:12 Point Resolved",
            "src/shop.cpp:12 Point Resolved",
            "src/shop.cpp:13 Point Resolved",
            "src/shop.cpp:13 Point Resolved",
            "src/shop.cpp:15 Point Resolved",
            "src/shop.cpp:8 Point Resolved",
        ]
    );
}
