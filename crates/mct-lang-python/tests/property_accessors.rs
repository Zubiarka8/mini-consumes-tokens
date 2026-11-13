//! A property is one `field` of its class, as in C# and Kotlin: the
//! `@property` (or `@cached_property`, `@functools.cached_property`)
//! getter declares it, with the getter's `def` as signature, and an
//! `@x.setter`, `@x.deleter` or `@x.getter` of the same name in the same
//! class adds an accessor to it, not a second row. The property owns the
//! calls in every accessor and spans them all; an accessor decorator names
//! the property's own attribute, so it references nothing. A setter of a
//! property the class does not declare (`@Base.x.setter`, an inherited
//! one) stays a method.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_python::PythonParser;

const SOURCE: &str = r#"class Account(Base):
    def __init__(self):
        self._balance = 0

    @property
    def balance(self) -> int:
        return audit(self._balance)

    @balance.setter
    def balance(self, value: int) -> None:
        self._balance = check(value)

    @balance.deleter
    def balance(self):
        reset()

    @functools.cached_property
    def report(self):
        return build()

    @owner.setter
    def owner(self, value):
        store(value)

    def show(self):
        self.balance = 3
        return self.balance
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&PythonParser, "bank/account.py", "Python", SOURCE)
}

#[test]
fn a_property_is_one_field_spanning_its_accessors() {
    let parsed = parsed();
    let members: Vec<(&str, SymbolKind, u32, Option<u32>)> = parsed
        .symbols
        .iter()
        .filter(|s| s.parent.as_deref() == Some("Account"))
        .map(|s| {
            (
                s.name.as_str(),
                s.kind,
                s.location.line,
                s.location.end_line,
            )
        })
        .collect();
    assert_eq!(
        members,
        [
            ("__init__", SymbolKind::Method, 2, Some(3)),
            ("_balance", SymbolKind::Field, 3, Some(3)),
            ("balance", SymbolKind::Field, 6, Some(15)),
            ("report", SymbolKind::Field, 18, Some(19)),
            // No `owner` property in this class: an inherited one.
            ("owner", SymbolKind::Method, 22, Some(23)),
            ("show", SymbolKind::Method, 25, Some(27)),
        ]
    );
    let balance = parsed.symbols.iter().find(|s| s.name == "balance").unwrap();
    let detail = parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == balance.id)
        .unwrap();
    assert_eq!(
        detail.signature.as_deref(),
        Some("def balance(self) -> int")
    );
}

#[test]
fn a_property_owns_its_accessors_calls_and_no_accessor_is_referenced() {
    let parsed = parsed();
    let name_of = |id| {
        let s = parsed.symbols.iter().find(|s| s.id == id).unwrap();
        format!("{} {:?}", s.name, s.kind)
    };
    let mut relations: Vec<(String, RelationKind, &str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| matches!(r.kind, RelationKind::Calls | RelationKind::References))
        .filter(|r| r.from != 0)
        .map(|r| (name_of(r.from), r.kind, r.to_name.as_str(), r.location.line))
        .collect();
    relations.sort_by_key(|r| r.3);
    let rel = |from: &str, kind, to, line| (from.to_string(), kind, to, line);
    assert_eq!(
        relations,
        [
            rel("balance Field", RelationKind::References, "property", 5),
            rel("balance Field", RelationKind::Calls, "audit", 7),
            rel("balance Field", RelationKind::Calls, "check", 11),
            rel("balance Field", RelationKind::Calls, "reset", 15),
            rel(
                "report Field",
                RelationKind::References,
                "cached_property",
                17
            ),
            rel("report Field", RelationKind::Calls, "build", 19),
            rel("owner Method", RelationKind::References, "setter", 21),
            rel("owner Method", RelationKind::Calls, "store", 23),
        ]
    );
}
