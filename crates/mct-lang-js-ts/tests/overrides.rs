//! D8 (issue #349): a TypeScript `override` method overrides a method of
//! the class its class `extends`. TypeScript accepts `override` only in a
//! class that extends another class, and an interface member is
//! implemented, not overridden, so the `extends` target is the one
//! candidate; there is no implicit root. A method without `override` gets
//! no relation, whatever its name, and neither does an `override` in a
//! class with no `extends` (a compile error) or in a class expression the
//! parser doesn't record as a class.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"
interface Shape {
    area(): number;
}
abstract class Base {
    abstract run(): void;
    name(): string { return "b"; }
}
class Square extends Base implements Shape {
    area(): number { return 1; }
    override run(): void {}
    override name(): string { return "s"; }
}
class Deep extends ns.Square {
    override area(): number { return 2; }
}
class Plain {
    override toString(): string { return "p"; }
}
class Outer extends Base {
    override run(): void {
        const C = class extends Plain {
            override toString(): string { return "c"; }
        };
    }
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(
        &JsTsParser,
        "src/shapes.ts",
        "JavaScript/TypeScript",
        SOURCE,
    )
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
fn an_override_names_the_extended_class_and_nothing_else() {
    assert_eq!(
        overrides(&parsed()),
        [
            "Square.run -> run (Base)",
            "Square.name -> name (Base)",
            "Deep.area -> area (Square)",
            "Outer.run -> run (Base)",
        ]
    );
}

#[test]
fn the_override_resolves_to_the_extended_class_method() {
    let root = std::env::temp_dir().join(format!("mct-ts-overrides-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/shapes.ts"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    let mut hits: Vec<String> = ["run", "name", "area"]
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
            "area Resolved Square:10",
            "name Resolved Base:7",
            "run Resolved Base:6",
            "run Resolved Base:6",
        ]
    );
}
