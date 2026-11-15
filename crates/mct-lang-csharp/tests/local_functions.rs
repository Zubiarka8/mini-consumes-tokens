//! A local function belongs to the method (or local function) it is
//! declared in: its `parent` is that function's name, as Python and JS/TS
//! nested functions have. It stays a `function`, and a method of a type
//! keeps the type as its parent.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"
class Shop
{
    public Shop()
    {
        int Setup() => 1;
    }

    public int Total(int[] xs)
    {
        int Twice(int v)
        {
            int Inner(int w) => w * 2;
            return Inner(v);
        }
        Func<int, int> f = v => { int InLambda() => v; return InLambda(); };
        return xs.Sum(Twice) + f(1);
    }
}
"#;

fn parsed() -> mct_core::ParsedFile {
    mct_corpus::parse_source(&CSharpParser, "src/Shop.cs", "C#", SOURCE)
}

#[test]
fn a_local_function_has_its_enclosing_function_as_parent() {
    let parsed = parsed();
    let parent_of = |name: &str| {
        let symbol = parsed
            .symbols
            .iter()
            .find(|s| s.name == name && s.kind == SymbolKind::Function)
            .unwrap_or_else(|| panic!("{name} missing"));
        symbol.parent.as_deref()
    };
    assert_eq!(parent_of("Twice"), Some("Total"));
    assert_eq!(parent_of("Inner"), Some("Twice"));
    // A named lambda is itself an indexed function and owns its locals.
    assert_eq!(parent_of("InLambda"), Some("f"));
    // A constructor is a method named after its class.
    assert_eq!(parent_of("Setup"), Some("Shop"));
}

#[test]
fn methods_keep_their_type_and_calls_to_local_functions_stay() {
    let parsed = parsed();
    let total = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Total" && s.kind == SymbolKind::Method)
        .unwrap();
    assert_eq!(total.parent.as_deref(), Some("Shop"));
    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    for callee in ["Inner", "InLambda"] {
        assert!(calls.contains(&callee), "{callee} missing from {calls:?}");
    }
}
