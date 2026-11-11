//! D8 (issue #349): a method decorated with `typing.override` (PEP 698)
//! overrides a method of one of its class's bases. Python allows several
//! bases and the syntax can't say which one declares the method, so each
//! base gets a relation qualified by it, and only a base that really
//! declares a same-named method resolves. A class with no base has the
//! root `object`, external, but only for the methods `object` defines.
//!
//! The decorator needs evidence that it is `typing`'s: a bare `override`
//! counts only when every binding of it is an import from `typing` or
//! `typing_extensions`, and a dotted one only through a module alias of
//! those. A user-defined `override`, or a method without the decorator,
//! gets no relation, whatever its name.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_python::PythonParser;

const SOURCE: &str = r#"
try:
    from typing import override
except ImportError:
    from typing_extensions import override
import typing_extensions as te


class Shape:
    def area(self) -> float: ...


class Named:
    def name(self) -> str: ...


class Square(Shape, Named):
    @override
    def area(self) -> float:
        return 1.0

    @override
    def name(self) -> str:
        return "s"

    def run(self) -> None: ...


class Deep(shapes.Square):
    @te.override
    def area(self) -> float:
        return 2.0


class Plain:
    @override
    def __repr__(self) -> str:
        return "p"

    @override
    def count(self) -> int:
        return 0


class Boxed(Base[int], metaclass=Meta):
    @override
    def run(self) -> None:
        def area() -> float:
            return 0.0
"#;

const USER_DECORATOR: &str = r#"
def override(f):
    return f


class Shape:
    def area(self) -> float: ...


class Square(Shape):
    @override
    def area(self) -> float:
        return 1.0
"#;

fn parse(source: &str) -> ParsedFile {
    mct_corpus::parse_source(&PythonParser, "src/shapes.py", "Python", source)
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
fn a_typing_override_names_every_base_and_nothing_else() {
    assert_eq!(
        overrides(&parse(SOURCE)),
        [
            "Square.area -> area (Shape)",
            "Square.area -> area (Named)",
            "Square.name -> name (Shape)",
            "Square.name -> name (Named)",
            "Deep.area -> area (Square)",
            "Plain.__repr__ -> __repr__ (object, external)",
            "Boxed.run -> run (Base)",
        ]
    );
}

/// `override` from a module that only ends in `typing`.
const OTHER_TYPING: &str = r#"
from app.typing import override


class Square(Shape):
    @override
    def area(self) -> float:
        return 1.0
"#;

/// A base the syntax can't name still rules out the `object` root.
const UNNAMED_BASE: &str = r#"
from typing import override


class Square(make_base()):
    @override
    def __repr__(self) -> str:
        return "s"
"#;

#[test]
fn a_user_defined_override_decorator_is_not_evidence() {
    assert_eq!(overrides(&parse(USER_DECORATOR)), Vec::<String>::new());
    assert_eq!(overrides(&parse(OTHER_TYPING)), Vec::<String>::new());
}

#[test]
fn a_base_the_syntax_cannot_name_is_not_the_object_root() {
    assert_eq!(overrides(&parse(UNNAMED_BASE)), Vec::<String>::new());
}

#[test]
fn only_the_base_declaring_the_method_resolves() {
    let root = std::env::temp_dir().join(format!("mct-python-overrides-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/shapes.py"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PythonParser));
    let mut hits: Vec<String> = ["area", "name", "__repr__", "run"]
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
            "__repr__ External ",
            "area Resolved Shape:10",
            "area Resolved Square:19",
            "area Unresolved ",
            "name Resolved Named:14",
            "name Unresolved ",
            "run Unresolved ",
        ]
    );
}
