//! D8 (issue #349): an `override` method overrides a `virtual` or
//! `abstract` method of its type's base class. C# puts the base class first
//! in the base list, and only a class can be overridden this way (an
//! interface member is implemented, not overridden), so the one candidate
//! supertype is the base list's first entry. A class with no base class has
//! the root `object` (`ValueType` for a struct), external, but only for the
//! methods `object` lets a subclass override (`Equals`, `GetHashCode`,
//! `ToString`). When the first entry is named like an interface (`IShape`)
//! both it and the root are candidates, since the syntax alone can't tell an
//! interface from a class. A method without `override` gets no relation,
//! whatever its name.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"
public interface IShape { double Area(); }
public abstract class Base { public abstract void Run(); public virtual string Name() => "b"; }
public class Square : Base, IShape {
    public double Area() => 1;
    public override void Run() {}
    public override string Name() => "s";
}
public class Plain {
    public override string ToString() => "p";
    public override int Count() => 0;
}
public class Tagged : IShape {
    public double Area() => 2;
    public override string ToString() => "t";
}
public struct Point {
    public override int GetHashCode() => 1;
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CSharpParser, "src/Shapes.cs", "C#", SOURCE)
}

/// `Parent.from -> to (qualifier[, external])` for every `Overrides`.
fn overrides(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Overrides)
        .map(|(i, r)| {
            let from = parsed.symbols.iter().find(|s| s.id == r.from).unwrap();
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            let qualifier = target.and_then(|t| t.qualifier.clone()).unwrap_or_default();
            let external = if target.is_some_and(|t| t.external) {
                ", external"
            } else {
                ""
            };
            format!(
                "{}.{} -> {} ({qualifier}{external})",
                from.parent.as_deref().unwrap_or(""),
                from.name,
                r.to_name
            )
        })
        .collect()
}

#[test]
fn an_override_names_the_base_class_and_nothing_else() {
    assert_eq!(
        overrides(&parsed()),
        [
            "Square.Run -> Run (Base)",
            "Square.Name -> Name (Base)",
            "Plain.ToString -> ToString (object, external)",
            "Tagged.ToString -> ToString (IShape)",
            "Tagged.ToString -> ToString (object, external)",
            "Point.GetHashCode -> GetHashCode (ValueType, external)",
        ]
    );
}

#[test]
fn only_the_base_class_declaring_the_method_resolves() {
    let root = std::env::temp_dir().join(format!("mct-csharp-overrides-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Shapes.cs"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CSharpParser));
    let mut hits: Vec<String> = ["Run", "Name", "ToString", "GetHashCode", "Area"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "overrides")
        .map(|h| {
            let target = h
                .target_id
                .and_then(|id| index.symbols_by_ids(&[id]).unwrap().pop())
                .map(|s| format!("{}:{}", s.parent.unwrap_or_default(), s.line))
                .unwrap_or_default();
            format!("{} {:?} {target}", h.to_name, h.resolution)
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "GetHashCode External ",
            "Name Resolved Base:3",
            "Run Resolved Base:3",
            "ToString External ",
            "ToString External ",
            "ToString Unresolved ",
        ]
    );
}
