//! `nameof(Member)` is resolved by the compiler, so it uses the member it
//! names: it is a `references` relation, not a call. A `nameof` that names a
//! qualified member (`nameof(Cart.Total)`) uses the last segment.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::RelationKind;
use mct_lang_csharp::CSharpParser;

fn relations(source: &str, kind: RelationKind) -> Vec<(String, String)> {
    let parsed = mct_corpus::parse_source(&CSharpParser, "src/Cart.cs", "C#", source);
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| {
            let from = parsed
                .symbols
                .iter()
                .find(|s| s.id == r.from)
                .map_or(String::new(), |s| s.name.clone());
            (from, r.to_name.clone())
        })
        .collect()
}

const SOURCE: &str = "class Cart {\n  int Total() => 1;\n  string Name = \"\";\n  \
    string Label() => nameof(Total) + nameof(Cart.Name) + nameof(Cart);\n}\n";

#[test]
fn nameof_references_the_member_it_names() {
    let found = relations(SOURCE, RelationKind::References);
    for target in ["Total", "Name", "Cart"] {
        assert!(
            found.contains(&("Label".into(), target.into())),
            "Label -> {target} missing in {found:?}"
        );
    }
}

#[test]
fn nameof_is_not_a_call() {
    let found = relations(SOURCE, RelationKind::Calls);
    assert!(
        !found.iter().any(|(_, to)| to == "nameof"),
        "unexpected nameof call in {found:?}"
    );
}
