//! A type written as a template argument (`Box<Tag>`,
//! `std::vector<std::pair<Row, Tag>>`) is a use of it, with the evidence a
//! declared type gets. A template parameter and a built-in type give none.

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::RelationKind;
use mct_lang_cpp::CppParser;

const SOURCE: &str = r#"#include <vector>
#include <utility>
struct Row {};
struct Tag {};
struct Key {};
template <typename T> struct Box { T v; };
template <typename T> struct Wrap { Box<T> inner; };
struct Holder {
  Box<Tag> tag;
  std::vector<std::pair<Row, int>> rows;
  Box<Box<Key>> nested;
};
"#;

#[test]
fn a_template_argument_type_is_referenced() {
    let parsed = mct_corpus::parse_source(&CppParser, "src/a.cpp", "C++", SOURCE);
    let mut refs: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| (r.to_name.as_str(), r.location.line))
        .collect();
    refs.sort_unstable();
    assert_eq!(
        refs,
        [
            ("Box", 7),
            ("Box", 9),
            ("Box", 11),
            ("Box", 11),
            ("Key", 11),
            ("Row", 10),
            ("Tag", 9),
            ("pair", 10),
            ("vector", 10),
        ]
    );
}
