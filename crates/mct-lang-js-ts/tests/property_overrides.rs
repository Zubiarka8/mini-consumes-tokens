//! D8 (issue #349): an `override` class field overrides a member of the
//! class its class `extends`, like an `override` method does. A field whose
//! value is an arrow function or a function expression is recorded as a
//! method, so it targets a method; any other field targets a same-named
//! field. A field without `override` gets no relation.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"
abstract class Base {
    abstract size: number;
    label: string = "b";
    run = (): void => {};
}
class Square extends Base {
    override size = 1;
    override label: string = "s";
    override run = (): void => {};
    extra = 2;
}
class Plain {
    override size = 3;
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
fn an_override_field_names_the_extended_class() {
    assert_eq!(
        overrides(&parsed()),
        [
            "Square.size -> size (Base)",
            "Square.label -> label (Base)",
            "Square.run -> run (Base)",
        ]
    );
}

#[test]
fn the_override_resolves_to_the_extended_class_member() {
    let root = std::env::temp_dir().join(format!("mct-ts-props-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/shapes.ts"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    let mut hits: Vec<String> = ["size", "label", "run"]
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
            "label Resolved Base.field:4",
            "run Resolved Base.method:5",
            "size Resolved Base.field:3",
        ]
    );
}
