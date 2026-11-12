//! `super(…)` in a constructor calls the constructor of the class the
//! enclosing class declaration extends, so it is `calls Base`, as
//! `new Base()` is. Only a base the source names (`Base`, `ns.Base`,
//! `Base<T>`) gives one: a mixin call (`extends mix(A)`) returns a class the
//! parser can't name, and a class expression has no declaration to read.
//! `super.m()` stays a call of `m`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_js_ts::JsTsParser;

const TS: &str = r#"
class Base { constructor(n: number) {} m() {} }
class Child extends Base {
    constructor() { super(1); super.m(); }
}
class Deep extends ns.Inner<number> {
    constructor() { super(); }
}
class Mixed extends mix(Base) {
    constructor() { super(); }
}
class Outer extends Base {
    constructor() {
        super();
        const C = class extends Other { constructor() { super(); } };
    }
}
"#;

// Plain JS: `class_heritage` wraps the base expression directly.
const JS: &str = r#"
class Base { constructor(n) {} m() {} }
class Child extends Base {
    constructor() { super(1); super.m(); }
}
class Deep extends ns.Inner {
    constructor() { super(); }
}
class Mixed extends mix(Base) {
    constructor() { super(); }
}
class Outer extends Base {
    constructor() {
        super();
        const C = class extends Other { constructor() { super(); } };
    }
}
"#;

fn parse(path: &str, source: &str) -> ParsedFile {
    mct_corpus::parse_source(&JsTsParser, path, "JavaScript/TypeScript", source)
}

/// `Parent.from -> to` for every `Calls` from a constructor.
fn constructor_calls(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .filter_map(|r| {
            let from = parsed.symbols.iter().find(|s| s.id == r.from)?;
            (from.name == "constructor").then(|| {
                format!(
                    "{}.constructor -> {}",
                    from.parent.as_deref().unwrap_or(""),
                    r.to_name
                )
            })
        })
        .collect()
}

const EXPECTED: [&str; 4] = [
    "Child.constructor -> Base",
    "Child.constructor -> m",
    "Deep.constructor -> Inner",
    "Outer.constructor -> Base",
];

#[test]
fn super_calls_the_extended_class_in_typescript() {
    assert_eq!(constructor_calls(&parse("src/shapes.ts", TS)), EXPECTED);
}

#[test]
fn super_calls_the_extended_class_in_javascript() {
    assert_eq!(constructor_calls(&parse("src/shapes.js", JS)), EXPECTED);
}

#[test]
fn super_of_an_unbound_standard_global_is_external() {
    let parsed = parse(
        "src/errors.ts",
        "class Oops extends Error { constructor() { super(\"x\"); } }\n",
    );
    let (index, _) = parsed
        .relations
        .iter()
        .enumerate()
        .find(|(_, r)| r.kind == RelationKind::Calls && r.to_name == "Error")
        .unwrap();
    assert!(parsed
        .relation_targets
        .iter()
        .any(|t| t.relation == index && t.external));
}
