//! D8 (issue #349): a PHP 8.3 method with the `#[\Override]` attribute
//! overrides a method of its class's parent class or of an interface it
//! implements (an interface's: of an interface it extends). The syntax
//! can't say which one declares the method, so each gets a relation
//! qualified by it, and only one that declares a same-named method
//! resolves. PHP has no root class.
//!
//! The attribute counts only when its name resolves to the global
//! `Override`: inside `namespace App`, a bare `#[Override]` with no `use
//! Override;` is `App\Override`, another attribute. A trait's method gets
//! no relation (PHP checks it in the class that uses the trait), nor does a
//! method without the attribute, whatever its name.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_php::PhpParser;

const SOURCE: &str = r#"<?php
namespace App;

use Override;

interface Shape {
    public function area(): float;
}
interface Solid extends Shape {
    #[\Override]
    public function area(): float;
}
abstract class Base {
    abstract public function run(): void;
}
class Square extends Base implements Shape {
    #[Override]
    public function area(): float { return 1.0; }
    #[\Override]
    public function run(): void {}
    public function name(): string { return "s"; }
}
trait Loud {
    #[\Override]
    public function run(): void {}
}
"#;

const NAMESPACED_ATTRIBUTE: &str = r#"<?php
namespace App;

class Base {
    public function run(): void {}
}
class Square extends Base {
    #[Override]
    public function run(): void {}
}
"#;

fn parse(source: &str) -> ParsedFile {
    mct_corpus::parse_source(&PhpParser, "src/Shapes.php", "PHP", source)
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
fn an_override_attribute_names_the_parent_and_the_interfaces() {
    assert_eq!(
        overrides(&parse(SOURCE)),
        [
            "Solid.area -> area (Shape)",
            "Square.area -> area (Base)",
            "Square.area -> area (Shape)",
            "Square.run -> run (Base)",
            "Square.run -> run (Shape)",
        ]
    );
}

#[test]
fn a_namespaced_override_attribute_is_not_evidence() {
    assert_eq!(
        overrides(&parse(NAMESPACED_ATTRIBUTE)),
        Vec::<String>::new()
    );
}

#[test]
fn only_the_supertype_declaring_the_method_resolves() {
    let root = std::env::temp_dir().join(format!("mct-php-overrides-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Shapes.php"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PhpParser));
    let mut hits: Vec<String> = ["area", "run"]
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
            "area Resolved Shape:7",
            "area Resolved Shape:7",
            "area Unresolved ",
            "run Resolved Base:14",
            "run Unresolved ",
        ]
    );
}
