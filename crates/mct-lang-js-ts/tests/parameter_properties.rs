//! A TypeScript constructor parameter with an accessibility keyword or
//! `readonly` (`constructor(private readonly store: Store)`) also declares
//! a property of the class, so it is a `field` of the class at the
//! parameter, with its visibility and `readonly`. A plain parameter, or one
//! of another method, declares nothing. The class declares that field, so
//! `this.clock()` reaches it and `this.owner.greet()` reaches the
//! parameter's declared type.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{Modifier, ParsedFile, RelationKind, SymbolKind, Visibility};
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"class Cart {
  constructor(
    private readonly store: Store,
    public size = 0,
    protected owner?: User,
    readonly id: string,
    plain: number,
  ) {}
  add(n: number) {}
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JsTsParser, "src/cart.ts", "TypeScript", SOURCE)
}

/// Name, parent, line, visibility, modifiers and signature of a field.
type FieldRow = (
    String,
    Option<String>,
    u32,
    Option<Visibility>,
    Vec<Modifier>,
    Option<String>,
);

#[test]
fn a_parameter_property_is_a_field_of_the_class() {
    let parsed = parsed();
    let fields: Vec<FieldRow> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Field)
        .map(|s| {
            let detail = parsed
                .symbol_details
                .iter()
                .find(|d| d.symbol == s.id)
                .unwrap();
            (
                s.name.clone(),
                s.parent.clone(),
                s.location.line,
                detail.visibility,
                detail.modifiers.clone(),
                detail.signature.clone(),
            )
        })
        .collect();
    let cart = Some("Cart".to_string());
    assert_eq!(
        fields,
        [
            (
                "store".to_string(),
                cart.clone(),
                3,
                Some(Visibility::Private),
                vec![Modifier::Readonly],
                Some("private readonly store: Store".to_string()),
            ),
            (
                "size".to_string(),
                cart.clone(),
                4,
                Some(Visibility::Public),
                vec![],
                Some("public size".to_string()),
            ),
            (
                "owner".to_string(),
                cart.clone(),
                5,
                Some(Visibility::Protected),
                vec![],
                Some("protected owner?: User".to_string()),
            ),
            (
                "id".to_string(),
                cart,
                6,
                Some(Visibility::Public),
                vec![Modifier::Readonly],
                Some("readonly id: string".to_string()),
            ),
        ]
    );
}

const CALLS: &str = r#"class Timer {
  constructor(private readonly clock: () => number, protected owner?: User) {}
  tick() {
    this.clock();
    this.owner.greet();
  }
}
"#;

#[test]
fn a_call_through_this_reaches_the_parameter_property_or_its_type() {
    let parsed = mct_corpus::parse_source(&JsTsParser, "src/timer.ts", "TypeScript", CALLS);
    let qualified: Vec<(String, Option<String>)> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .map(|(i, r)| {
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            (r.to_name.clone(), target.and_then(|t| t.qualifier.clone()))
        })
        .collect();
    assert_eq!(
        qualified,
        [
            ("clock".to_string(), Some("Timer".to_string())),
            ("greet".to_string(), Some("User".to_string())),
        ]
    );
}
