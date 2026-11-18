//! A type named by a cast (`static_cast<Marker*>(p)`, `(Marker*)p`) or by
//! the target of `using Name = Type;` is a use of it, with the evidence a
//! declared type gets: a type of this file is pinned to it, a `std::` type
//! is external, a built-in type and a template parameter name none.

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::RelationKind;
use mct_lang_cpp::CppParser;

const SOURCE: &str = r#"struct Marker {};
struct Alias {};
struct Other {};
struct Fourth {};
using AliasT = Alias;
template <typename T> T* as(void* p) { return static_cast<T*>(p); }
bool check(void* o) {
  auto a = dynamic_cast<Marker*>(static_cast<Other*>(o));
  auto b = (Fourth*)o;
  int c = static_cast<int>(1);
  return a != nullptr && b != nullptr && c;
}
"#;

#[test]
fn a_cast_or_alias_target_references_its_type() {
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
        [("Alias", 5), ("Fourth", 9), ("Marker", 8), ("Other", 8)]
    );
}
