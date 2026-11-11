//! D8 (issue #349): an `override val`/`override var`, in a class body or
//! as a constructor property, overrides a property of a direct supertype,
//! like an `override fun` does a function. The target is a same-named
//! property (a `Field`) of that supertype. `Any` declares no property, so
//! there is no root, and a property without `override` gets no relation.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_kotlin::KotlinParser;

const SOURCE: &str = r#"
interface Shape {
    val area: Double
    fun name(): String
}
abstract class Base {
    abstract var size: Int
}
class Square(override val area: Double) : Base(), Shape {
    override var size: Int = 1
    val name: String = "s"
    override fun name(): String = "s"
}
class Plain(override val id: Int) {
    override val tag: String = "p"
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/Shapes.kt", "Kotlin", SOURCE)
}

/// `Parent.from -> to (qualifier)` for every `Overrides` from a property.
fn property_overrides(parsed: &ParsedFile) -> Vec<String> {
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
                "{:?} {}.{} -> {} ({qualifier})",
                from.kind,
                from.parent.as_deref().unwrap_or(""),
                from.name,
                r.to_name
            )
        })
        .filter(|line| line.starts_with("Field"))
        .collect()
}

#[test]
fn an_override_property_names_every_direct_supertype() {
    assert_eq!(
        property_overrides(&parsed()),
        [
            "Field Square.area -> area (Base)",
            "Field Square.area -> area (Shape)",
            "Field Square.size -> size (Base)",
            "Field Square.size -> size (Shape)",
        ]
    );
}

#[test]
fn the_override_resolves_to_the_supertype_property() {
    let root = std::env::temp_dir().join(format!("mct-kotlin-props-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Shapes.kt"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(KotlinParser));
    let mut hits: Vec<String> = ["area", "size"]
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
            "area Resolved Shape.field:3",
            "area Unresolved ",
            "size Resolved Base.field:7",
            "size Unresolved ",
        ]
    );
}
