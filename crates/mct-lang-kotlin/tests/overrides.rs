//! D8 (issue #349): an `override fun` overrides a function of a direct
//! supertype named in its type's delegation specifiers. The parser can't
//! tell which one declares it, so there is one `Overrides` relation per
//! direct supertype, each qualified by it: only the supertype that declares
//! the function resolves. A type that extends no class also has the root
//! `Any` (`Enum` for an enum class), external, but only for the functions
//! `Any` lets a subclass override (`equals`, `hashCode`, `toString`). A
//! function without `override` gets no relation, whatever its name.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_kotlin::KotlinParser;

const SOURCE: &str = r#"
interface Shape {
    fun area(): Double
}
interface Named {
    fun name(): String
}
abstract class Base {
    abstract fun run()
}
class Square : Base(), Shape, Named {
    override fun area(): Double = 1.0
    override fun run() {}
    fun name(): String = "s"
}
class Plain {
    override fun toString(): String = "p"
}
class Pt : Comparable<Pt> {
    override fun compareTo(other: Pt): Int = 0
    override fun hashCode(): Int = 1
}
class Del(s: Shape) : Shape by s {
    override fun area(): Double = 2.0
}
enum class Op {
    ADD {
        override fun apply(a: Int): Int = a
    };

    abstract fun apply(a: Int): Int
}
class Use {
    fun start() {
        val r = object : Runnable {
            override fun run() {}
        }
    }
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/Shapes.kt", "Kotlin", SOURCE)
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
            "Plain.toString -> toString (Any, external)",
            "Pt.compareTo -> compareTo (Comparable)",
            "Pt.hashCode -> hashCode (Comparable)",
            "Pt.hashCode -> hashCode (Any, external)",
            "Del.area -> area (Shape)",
            "Op.apply -> apply (Op)",
            ".run -> run (Runnable)",
        ]
    );
}

/// `Op.apply` is left out: the function of an enum entry's body has the
/// enum as its parent, so it is itself a candidate for `apply (Op)` and the
/// relation is ambiguous between it and the abstract function. A known
/// limit, recorded in the phase 2-3 tracker.
#[test]
fn only_the_supertype_declaring_the_function_resolves() {
    let root = std::env::temp_dir().join(format!("mct-kotlin-overrides-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Shapes.kt"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(KotlinParser));
    let mut hits: Vec<String> = ["area", "run", "toString", "hashCode"]
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
            "area Resolved Shape:3",
            "area Resolved Shape:3",
            "hashCode External ",
            "run Resolved Base:9",
            "toString External ",
        ]
    );
}
