//! A `const` member is a compile-time constant, so it is a `constant` of
//! its type (as Rust's `const`, C++'s `constexpr` and Kotlin's `const val`
//! are), one per declarator. `static readonly` and `readonly` are fields:
//! their value is set at run time.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::SymbolKind;
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"
class Limits
{
    public const int Max = 10, Min = 1;
    private const string Name = "n";
    public static readonly int Cap = 5;
    public readonly int Slots = 2;
    public int Count;
    void Run() { const int Local = 3; }
}
"#;

fn kinds() -> Vec<(String, SymbolKind, Option<String>)> {
    mct_corpus::parse_source(&CSharpParser, "src/Limits.cs", "C#", SOURCE)
        .symbols
        .into_iter()
        .filter(|s| matches!(s.kind, SymbolKind::Constant | SymbolKind::Field))
        .map(|s| (s.name, s.kind, s.parent))
        .collect()
}

#[test]
fn a_const_member_is_a_constant_of_its_type() {
    let found = kinds();
    for name in ["Max", "Min", "Name"] {
        assert!(
            found.contains(&(
                name.to_string(),
                SymbolKind::Constant,
                Some("Limits".to_string())
            )),
            "{name} should be a constant: {found:?}"
        );
    }
}

#[test]
fn run_time_members_stay_fields_and_locals_are_no_symbol() {
    let found = kinds();
    for name in ["Cap", "Slots", "Count"] {
        assert!(
            found
                .iter()
                .any(|(n, k, _)| n == name && *k == SymbolKind::Field),
            "{name} should be a field: {found:?}"
        );
    }
    assert!(found.iter().all(|(n, _, _)| n != "Local"), "{found:?}");
}
