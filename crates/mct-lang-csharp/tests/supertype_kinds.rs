//! A base-list entry names a type, so it can only reach a type: a
//! same-named constructor (`Base(int x)` next to `class Base`) is never the
//! target of `extends`/`implements`. The first entry of a class is its base
//! class unless it is named like an interface; every other entry, and every
//! entry of an interface or a struct, is an interface.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"
namespace Shop;

class Cart : Store.Basket<Item>, IDisposable, Marker { }
class Plain : IShape { }
interface Wide : IFirst, Second { }
struct Point : IShape { }
record Pair(int A) : Base(A), IShape;
"#;

/// `from -> to (relation, target kind)` for every supertype relation.
fn supertypes() -> Vec<String> {
    let parsed = mct_corpus::parse_source(&CSharpParser, "src/Cart.cs", "C#", SOURCE);
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r.kind, RelationKind::Extends | RelationKind::Implements))
        .map(|(i, r)| {
            let from = parsed
                .symbols
                .iter()
                .find(|s| s.id == r.from)
                .map_or("?", |s| s.name.as_str());
            let kind = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.kind);
            format!("{from} -> {} ({:?}, {kind:?})", r.to_name, r.kind)
        })
        .collect()
}

#[test]
fn a_supertype_can_only_be_a_type() {
    let found = supertypes();
    let class = Some(SymbolKind::Class);
    let interface = Some(SymbolKind::Interface);
    for expected in [
        format!("Cart -> Basket (Extends, {class:?})"),
        format!("Cart -> IDisposable (Implements, {interface:?})"),
        format!("Cart -> Marker (Implements, {interface:?})"),
        format!("Plain -> IShape (Implements, {interface:?})"),
        format!("Wide -> IFirst (Extends, {interface:?})"),
        format!("Wide -> Second (Extends, {interface:?})"),
        format!("Point -> IShape (Implements, {interface:?})"),
        format!("Pair -> Base (Extends, {class:?})"),
        format!("Pair -> IShape (Implements, {interface:?})"),
    ] {
        assert!(
            found.contains(&expected),
            "{expected} missing from {found:#?}"
        );
    }
}
