//! D8 (issue #349): an `@Override` method overrides a method of a direct
//! supertype written in its type's header. The parser can't tell which one
//! declares it, so there is one `Overrides` relation per direct supertype,
//! each qualified by it: only the supertype that declares the method
//! resolves. A type that extends no class also has the JDK root `Object`
//! (`Enum` for an enum, `Record` for a record), external, but only for the
//! methods `Object` lets a subclass override (`equals`, `hashCode`,
//! `toString`, `clone`, `finalize`). A method without `@Override` gets no
//! relation, whatever its name.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_java::JavaParser;

const SOURCE: &str = r#"
interface Shape { double area(); }
interface Named { String name(); }
abstract class Base { abstract void run(); }
class Square extends Base implements Shape, Named {
    @Override public double area() { return 1; }
    @Override void run() {}
    public String name() { return "s"; }
}
class Plain {
    @Override public boolean equals(Object o) { return true; }
}
class Pt implements Comparable<Pt> {
    @Override public int compareTo(Pt o) { return 0; }
    @Override public String toString() { return "pt"; }
}
enum Op {
    ADD { @Override int apply(int a) { return a; } };
    abstract int apply(int a);
}
class Use {
    void start() { Runnable r = new Runnable() { @Override public void run() {} }; }
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JavaParser, "src/Shapes.java", "Java", SOURCE)
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
fn an_override_names_every_direct_supertype_and_nothing_else() {
    assert_eq!(
        overrides(&parsed()),
        [
            "Square.area -> area (Base)",
            "Square.area -> area (Shape)",
            "Square.area -> area (Named)",
            "Square.run -> run (Base)",
            "Square.run -> run (Shape)",
            "Square.run -> run (Named)",
            "Plain.equals -> equals (Object, external)",
            "Pt.compareTo -> compareTo (Comparable)",
            "Pt.toString -> toString (Comparable)",
            "Pt.toString -> toString (Object, external)",
            "ADD.apply -> apply (Op)",
            "Use.run -> run (Runnable)",
        ]
    );
}

/// The method of an enum constant's body has the constant as its parent, so
/// `apply (Op)` has one candidate, the enum's abstract method, and never the
/// overriding method itself.
#[test]
fn only_the_supertype_declaring_the_method_resolves() {
    let root = std::env::temp_dir().join(format!("mct-java-overrides-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Shapes.java"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JavaParser));
    let mut hits: Vec<String> = ["area", "run", "equals", "toString", "apply"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "overrides" && h.resolution != mct_index::Resolution::Unresolved)
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
            "apply Resolved Op:19",
            "area Resolved Shape:2",
            "equals External ",
            "run Resolved Base:4",
            "toString External ",
        ]
    );
}
