//! D8 (issue #349): a C++ member function declared `override` overrides a
//! virtual function of one of its class's bases. C++ allows several bases
//! (an interface is just an abstract base) and the syntax can't say which
//! one declares the function, so each base gets a relation qualified by it
//! (`Base<T>` and `ns::Base` are `Base`), and only a base that declares a
//! same-named method resolves. C++ has no root class. `override` is only
//! written in the class body, so an out-of-line definition gets no
//! relation, and a function without `override` gets none, whatever its
//! name.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_cpp::CppParser;

const SOURCE: &str = r#"
struct Shape {
    virtual double area() const = 0;
};
class Named {
public:
    virtual const char* name() const;
};
template <typename T> class Base {
public:
    virtual void run();
};
class Square : public Shape, public Named, private Base<int> {
public:
    double area() const override { return 1.0; }
    const char* name() const override;
    void run() final;
    void stop();
    ~Square() override;
};
class Deep : public geo::Square {
    double area() const override final { return 2.0; }
};
class Plain {
    void run() override;
};
const char* Square::name() const { return "s"; }
void outer() {
    struct Local : Shape {
        double area() const override { return 0.0; }
    };
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CppParser, "src/shapes.cpp", "C++", SOURCE)
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
fn an_override_names_every_base_and_nothing_else() {
    assert_eq!(
        overrides(&parsed()),
        [
            "Square.area -> area (Shape)",
            "Square.area -> area (Named)",
            "Square.area -> area (Base)",
            "Square.name -> name (Shape)",
            "Square.name -> name (Named)",
            "Square.name -> name (Base)",
            "Deep.area -> area (Square)",
            "Local.area -> area (Shape)",
        ]
    );
}

#[test]
fn only_the_base_declaring_the_function_resolves() {
    let root = std::env::temp_dir().join(format!("mct-cpp-overrides-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/shapes.cpp"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CppParser));
    let mut hits: Vec<String> = ["area", "name"]
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
            "area Resolved Shape:3",
            "area Resolved Shape:3",
            "area Resolved Square:15",
            "area Unresolved ",
            "area Unresolved ",
            "name Resolved Named:7",
            "name Unresolved ",
            "name Unresolved ",
        ]
    );
}
