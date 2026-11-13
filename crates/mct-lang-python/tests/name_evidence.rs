//! What a name's syntax proves about its target, now that module variables
//! and class attributes are symbols. Python can't reach another module's
//! name without importing it, so a bare name no import binds, in a file
//! without `import *`, is this file's (or a local's, or a builtin's: then
//! it stays unresolved). `v.f()` on a value (a call result, a subscript, a
//! parameter or local no import binds) is a member call: a method of a type
//! the syntax doesn't prove, never a module-level name or a class
//! attribute. A module-level name may hold a module, so it proves nothing,
//! unless the file binds it once, at module level, to a builtin literal
//! (`TABLE = {}`): then its methods are the builtin's, `external`.
//! A `from m import x` names an `x` of module `m`, even where a class body
//! assigns a class attribute `x`, which rebinds nothing outside that body.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_python::PythonParser;

const SOURCE: &str = r#"import os
from django.db.models import F
from .logger import log
from .auth import permission_required

MODULE = load_module()
TABLE = {"a": 1}
TWICE = {}
TWICE = load_module()


def run(member, text):
    helper()
    log("start")
    os.path.join("a", "b")
    member.tier.name.title()
    text.strip()
    load().close()
    items[0].pop()
    Cart.create()
    F("price")
    MODULE.inspect()
    TABLE.items()
    TWICE.keys()


class Cart:
    permission_required = "add"

    @permission_required
    def add(self):
        pass


@permission_required
def remove():
    pass


@permission_required("delete")
def delete():
    pass
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&PythonParser, "shop/run.py", "Python", SOURCE)
}

/// `line kind name evidence` for every call and import.
fn evidence(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r.kind, RelationKind::Calls | RelationKind::Imports))
        .map(|(i, r)| {
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            let evidence = match target {
                None => "-".to_string(),
                Some(t) if t.member => "member".to_string(),
                Some(t) if t.external => "external".to_string(),
                Some(t) => match (&t.path, &t.module, &t.qualifier) {
                    (Some(path), _, _) => format!("path {path}"),
                    (_, Some(module), _) => format!("module {module}"),
                    (_, _, Some(class)) => format!("qualifier {class}"),
                    _ => "?".to_string(),
                },
            };
            format!("{} {:?} {} {evidence}", r.location.line, r.kind, r.to_name)
        })
        .collect()
}

#[test]
fn a_name_carries_what_its_syntax_proves() {
    let rows = evidence(&parsed());
    let expected = [
        "2 Imports F module models",
        "3 Imports log module logger",
        "4 Imports permission_required module auth",
        "13 Calls helper path shop/run.py",
        "14 Calls log module logger",
        "16 Calls title member",
        "17 Calls strip member",
        "18 Calls close member",
        "18 Calls load path shop/run.py",
        "19 Calls pop member",
        "20 Calls create qualifier Cart",
        "21 Calls F module models",
        "22 Calls inspect -",
        "23 Calls items external",
        "24 Calls keys -",
    ];
    for row in expected {
        assert!(rows.iter().any(|r| r == row), "{row} not in {rows:#?}");
    }
}

/// A decorator in the class body may be the class attribute; outside it,
/// the import.
#[test]
fn a_class_attribute_hides_an_import_only_in_its_class_body() {
    let parsed = parsed();
    let decorators: Vec<String> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.to_name == "permission_required" && r.kind == RelationKind::References)
        .map(|(i, r)| {
            let module = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.module.clone());
            format!("{} {}", r.location.line, module.as_deref().unwrap_or("-"))
        })
        .collect();
    assert_eq!(decorators, ["30 -", "35 auth", "40 auth"]);
}
