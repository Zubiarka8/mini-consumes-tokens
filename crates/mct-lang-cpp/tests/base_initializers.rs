//! A constructor's mem-initializer that names a base class runs that
//! base's constructor ([class.base.init]), as `Base(1)` in an expression
//! does: `Cart() : Base(1)` calls `Base`. One that names the class itself
//! delegates to another constructor of it, so `Cart(int) : Cart()` calls
//! `Cart`. A name that is neither is a member (`size_(3)`) and calls
//! nothing. The bases come from the class definition in the same file;
//! an out-of-line constructor of a class defined elsewhere gives no call,
//! because nothing tells a base from a member there. The target is a
//! constructor declared in the named class.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_cpp::CppParser;

const SOURCE: &str = r#"#include <stdexcept>
class Base { public: Base(int n) {} };
namespace ns { template <typename T> class Box { public: Box(T t) {} }; }
class Cart : public Base, public ns::Box<int> {
public:
    Cart() : Base(1), ns::Box<int>(2), size_(3) {}
    Cart(int n) : Cart() {}
    Cart(long n);
private:
    int size_;
};
Cart::Cart(long n) : Base{4}, size_(5) {}
class Error : public std::runtime_error {
    Error() : std::runtime_error("x") {}
};
class Elsewhere;
Elsewhere::Elsewhere() : Base(6) {}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CppParser, "src/cart.cpp", "C++", SOURCE)
}

/// `line Parent.from -> to` (plus ` external`) for every call from a
/// constructor.
fn constructor_calls(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .filter_map(|(i, r)| {
            let from = parsed.symbols.iter().find(|s| s.id == r.from)?;
            let parent = from.parent.as_deref()?;
            (from.name == parent).then(|| {
                let external = parsed
                    .relation_targets
                    .iter()
                    .any(|t| t.relation == i && t.external);
                format!(
                    "{} {parent}.{} -> {}{}",
                    r.location.line,
                    from.name,
                    r.to_name,
                    if external { " external" } else { "" }
                )
            })
        })
        .collect()
}

#[test]
fn a_base_or_delegating_initializer_calls_a_constructor() {
    assert_eq!(
        constructor_calls(&parsed()),
        [
            "6 Cart.Cart -> Base",
            "6 Cart.Cart -> Box",
            "7 Cart.Cart -> Cart",
            "12 Cart.Cart -> Base",
            "14 Error.Error -> runtime_error external",
        ]
    );
}

#[test]
fn the_target_is_a_constructor_declared_in_that_class() {
    let parsed = parsed();
    let targets: Vec<_> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .map(|(i, r)| {
            let t = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .unwrap();
            (
                r.to_name.as_str(),
                t.qualifier.as_deref(),
                t.kind,
                t.external,
            )
        })
        .collect();
    let method = Some(mct_core::SymbolKind::Method);
    assert_eq!(
        targets,
        [
            ("Base", Some("Base"), method, false),
            ("Box", Some("Box"), method, false),
            ("Cart", Some("Cart"), method, false),
            ("Base", Some("Base"), method, false),
            ("runtime_error", None, None, true),
        ]
    );
}
