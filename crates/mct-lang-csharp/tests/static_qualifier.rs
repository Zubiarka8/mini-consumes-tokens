//! A capitalized name that is no local or field, used as the qualifier of
//! a call (`Helpers.Format(x)`), names a type: the call also `references`
//! it, and only a `class` can be the target (`static` is a class-only
//! modifier). A local, field or `this` qualifier, and a lowercase name
//! (a namespace), reference nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::RelationKind;
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"namespace Shop;
public static class Helpers {
    public static string Format(int x) => x.ToString();
}
public class Cart {
    private Helpers Tools;
    public string Show(Cart other) {
        var Local = new Cart();
        Local.Show(other);
        Tools.Format(1);
        system.Run();
        return Helpers.Format(2);
    }
}
"#;

#[test]
fn a_type_qualifier_references_the_class() {
    let parsed = mct_corpus::parse_source(&CSharpParser, "Shop/Cart.cs", "C#", SOURCE);
    let refs: Vec<(&str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| (r.to_name.as_str(), r.location.line))
        .collect();
    assert_eq!(refs, [("Helpers", 12)]);
}

#[test]
fn the_reference_reaches_the_class_not_its_namesakes() {
    let root = std::env::temp_dir().join(format!("mct-cs-static-qualifier-{}", std::process::id()));
    std::fs::create_dir_all(root.join("Shop")).unwrap();
    std::fs::write(root.join("Shop/Cart.cs"), SOURCE).unwrap();
    // A method named like the class is never the target.
    std::fs::write(
        root.join("Shop/Other.cs"),
        "namespace Shop;\npublic class Other { public void Helpers() { } }\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CSharpParser));
    let hits: Vec<String> = index
        .find_references("Helpers")
        .unwrap()
        .into_iter()
        .filter(|h| h.kind == "references")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["Shop/Cart.cs:12 Resolved"]);
}
