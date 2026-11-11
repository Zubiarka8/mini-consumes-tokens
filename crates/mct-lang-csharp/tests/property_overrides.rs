//! D8 (issue #349): an `override` property, indexer or event overrides a
//! `virtual`/`abstract` one of the type's base class, like an `override`
//! method does. The target is a same-named member recorded as a `Field`
//! (an indexer is named `this`). `object` declares no property, so a type
//! with no base list gets no relation, and a member without `override`
//! gets none, whatever its name.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"
public abstract class Base {
    public abstract double Area { get; }
    public virtual string this[int i] => "b";
    public virtual event System.Action Changed;
    public virtual string Name { get; set; }
}
public class Square : Base {
    public override double Area => 1;
    public override string this[int i] => "s";
    public override event System.Action Changed;
    public string Name { get; set; }
}
public class Plain {
    public override int Count => 0;
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CSharpParser, "src/Shapes.cs", "C#", SOURCE)
}

/// `Parent.from -> to (qualifier)` for every `Overrides`.
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
            format!(
                "{}.{} -> {} ({qualifier})",
                from.parent.as_deref().unwrap_or(""),
                from.name,
                r.to_name
            )
        })
        .collect()
}

#[test]
fn an_override_property_names_the_base_class() {
    assert_eq!(
        overrides(&parsed()),
        [
            "Square.Area -> Area (Base)",
            "Square.this -> this (Base)",
            "Square.Changed -> Changed (Base)",
        ]
    );
}

#[test]
fn the_override_resolves_to_the_base_class_member() {
    let root = std::env::temp_dir().join(format!("mct-csharp-props-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Shapes.cs"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CSharpParser));
    let mut hits: Vec<String> = ["Area", "this", "Changed", "Name"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "overrides")
        .map(|h| {
            let target = h
                .target_id
                .and_then(|id| index.symbols_by_ids(&[id]).unwrap().pop())
                .map(|s| format!("{}.{}:{}", s.parent.unwrap_or_default(), s.kind, s.line))
                .unwrap_or_default();
            format!("{} {:?} {target}", h.to_name, h.resolution)
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "Area Resolved Base.field:3",
            "Changed Resolved Base.field:5",
            "this Resolved Base.field:4",
        ]
    );
}
