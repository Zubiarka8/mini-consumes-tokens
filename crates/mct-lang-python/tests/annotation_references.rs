//! A name used in a type annotation (`h: Handler`, `-> Out`, `rows:
//! list[Row]`, `x: A | None`) is a use of it: a `references` relation, with
//! the evidence a bare call to that name gets. A dotted name, a string
//! forward reference and an unbound name record nothing.

// Test code: panicking on a broken precondition is correct here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::RelationKind;
use mct_lang_python::PythonParser;

const SOURCE: &str = r#"from typing import Optional
from app.models import Row


class Handler:
    pass


class Config:
    pass


class Out:
    pass


class Maybe:
    pass


def run(h: Handler, c: Optional[Config] = None, rows: list[Row] = (), m: Maybe | None = None, q: "Quoted" = 1, u: Unknown = 2) -> Out:
    total: int = 0
    return total
"#;

#[test]
fn a_name_in_an_annotation_is_referenced_with_evidence() {
    let parsed = mct_corpus::parse_source(&PythonParser, "app/run.py", "Python", SOURCE);
    let mut names: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    names.sort_unstable();
    // `Optional` is an import and `int`/`list` builtins: all carry evidence.
    assert_eq!(
        names,
        ["Config", "Handler", "Maybe", "Optional", "Out", "Row", "int", "list"]
    );
}
