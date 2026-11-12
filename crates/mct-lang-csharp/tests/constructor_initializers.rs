//! A constructor's `: this(…)` / `: base(…)` initializer calls a
//! constructor: `this` of the enclosing type, `base` of the type the base
//! list names first (C# requires the base class first). The target is a
//! constructor declared in that type, so when the first entry is in fact an
//! interface nothing matches: unresolved, never wrong. A class with no base
//! list derives from `object`, so its `: base()` is external.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"
namespace Shop;

class Cart : Store.Basket<Item>, IDisposable
{
    public Cart() : this(1) { }
    public Cart(int n) : base(n) { }
}

class Plain
{
    public Plain() : base() { }
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CSharpParser, "src/Cart.cs", "C#", SOURCE)
}

/// `Parent.from -> to (qualifier, kind)` / `external` for every call from a
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
                let target = parsed.relation_targets.iter().find(|t| t.relation == i);
                let evidence = match target {
                    Some(t) if t.external => "external".to_string(),
                    Some(t) => format!("{:?} {:?}", t.qualifier, t.kind),
                    None => "-".to_string(),
                };
                format!("{parent}.{} -> {} {evidence}", from.name, r.to_name)
            })
        })
        .collect()
}

#[test]
fn this_and_base_initializers_call_a_constructor() {
    let method = Some(SymbolKind::Method);
    assert_eq!(
        constructor_calls(&parsed()),
        [
            format!("Cart.Cart -> Cart {:?} {method:?}", Some("Cart")),
            format!("Cart.Cart -> Basket {:?} {method:?}", Some("Basket")),
            "Plain.Plain -> object external".to_string(),
        ]
    );
}
