//! A Java `extends`/`implements` names a type (JLS §8.1.4, §8.1.5, §9.1.3):
//! a class's `extends` a class, its `implements` and an interface's
//! `extends` an interface. A constructor or method spelled like the type is
//! never the target, through the real parse -> SQLite -> query pipeline.
//!
//! The project is written to a temporary directory rather than checked in
//! under `tests/fixtures/`, so its deliberately clashing names never become
//! candidates in this repository's own index. The files compile together
//! with `javac`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_java::JavaParser;

const BASE: &str = "\
package shop;

public abstract class Base {
    protected Base(int size) {}
}
";

const SHAPE: &str = "\
package shop;

public interface Shape {
    double area();
}
";

const NAMED: &str = "\
package shop;

public interface Named extends Shape {
    String name();
}
";

const FACTORY: &str = "\
package shop;

public class Factory {
    public Shape Shape() {
        return null;
    }
}
";

const SQUARE: &str = "\
package shop;

public class Square extends Base implements Named {
    public Square() {
        super(4);
    }

    public double area() {
        return 1;
    }

    public String name() {
        return getClass().getSimpleName();
    }
}
";

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-java-supertype-{}-{unique}",
        std::process::id()
    ));
    let dir = root.join("src/shop");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, contents) in [
        ("Base.java", BASE),
        ("Shape.java", SHAPE),
        ("Named.java", NAMED),
        ("Factory.java", FACTORY),
        ("Square.java", SQUARE),
    ] {
        std::fs::write(dir.join(name), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JavaParser));
    (index, root)
}

/// The one `kind` relation to `to` written in `file`.
fn relation(index: &Index, file: &str, kind: &str, to: &str) -> RelationHit {
    let hits: Vec<_> = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == file && h.kind == kind)
        .collect();
    assert_eq!(hits.len(), 1, "{kind} {to} in {file}: {hits:?}");
    hits.into_iter().next().unwrap()
}

/// `path:line` of the symbol `hit` resolved to.
fn target(index: &Index, hit: &RelationHit) -> String {
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    let id = hit.target_id.unwrap();
    let symbol = index
        .symbols_by_ids(&[id])
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    format!("{}:{}", symbol.relative_path, symbol.line)
}

#[test]
fn extends_reaches_the_class_never_its_constructor() {
    let (index, root) = indexed();
    let hit = relation(&index, "src/shop/Square.java", "extends", "Base");
    assert_eq!(target(&index, &hit), "src/shop/Base.java:3");
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn implements_reaches_the_interface_never_a_same_named_method() {
    let (index, root) = indexed();
    let hit = relation(&index, "src/shop/Square.java", "implements", "Named");
    assert_eq!(target(&index, &hit), "src/shop/Named.java:3");
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn an_interface_extends_its_superinterface() {
    let (index, root) = indexed();
    let hit = relation(&index, "src/shop/Named.java", "extends", "Shape");
    assert_eq!(target(&index, &hit), "src/shop/Shape.java:3");
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn annotations_on_supertypes_do_not_create_extra_inheritance() {
    use mct_core::RelationKind;

    let source = r#"
import java.lang.annotation.*;
@Target(ElementType.TYPE_USE) @interface Tag { Class<?> value(); }
class Other {}
class Base<T> {}
interface Parent<T> {}
interface Extra {}
interface Child extends @Tag(Other.class) Parent<String>, Extra {}
class Derived extends @Tag(Other.class) Base<String> implements @Tag(Other.class) Parent<String>, Extra {}
"#;
    let parsed = mct_corpus::parse_source(&JavaParser, "Example.java", "Java", source);
    let relations: Vec<_> = parsed
        .relations
        .iter()
        .filter(|relation| {
            matches!(
                relation.kind,
                RelationKind::Extends | RelationKind::Implements
            )
        })
        .map(|relation| (relation.kind, relation.to_name.as_str()))
        .collect();
    assert_eq!(
        relations,
        vec![
            (RelationKind::Extends, "Parent"),
            (RelationKind::Extends, "Extra"),
            (RelationKind::Extends, "Base"),
            (RelationKind::Implements, "Parent"),
            (RelationKind::Implements, "Extra"),
        ]
    );
}
