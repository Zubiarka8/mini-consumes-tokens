//! A class that subclasses a base of the standard `enum` module (`Enum`,
//! `IntEnum`, `StrEnum`, `Flag`, `IntFlag`) is an `enum`, and each member it
//! assigns stays a `field` of it, as a TS, Java or Kotlin enum member is.
//! The base counts only when the file proves it is the standard one: a bare
//! name only `from enum import …` binds, or the base on a module alias of
//! `enum`. A base named `Enum` that a relative import, a local class or a
//! later assignment binds proves nothing, so that class stays a `class`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::SymbolKind;
use mct_lang_python::PythonParser;

fn kinds(source: &str) -> Vec<(String, SymbolKind, Option<String>)> {
    let parsed = mct_corpus::parse_source(&PythonParser, "src/colors.py", "Python", source);
    parsed
        .symbols
        .iter()
        .filter(|s| !matches!(s.kind, SymbolKind::Module))
        .map(|s| (s.name.clone(), s.kind, s.parent.clone()))
        .collect()
}

fn row(name: &str, kind: SymbolKind, parent: Option<&str>) -> (String, SymbolKind, Option<String>) {
    (name.to_string(), kind, parent.map(str::to_string))
}

#[test]
fn a_subclass_of_a_standard_enum_base_is_an_enum() {
    let source = r#"import enum
import enum as e
from enum import Enum, IntFlag as Bits
class Color(Enum):
    RED = 1
    def label(self):
        return self.name
class Perm(Bits):
    R = 4
class Size(enum.StrEnum):
    S = "s"
class Mode(e.Flag):
    A = 1
class Plain(object):
    X = 1
"#;
    assert_eq!(
        kinds(source),
        [
            row("Color", SymbolKind::Enum, None),
            row("RED", SymbolKind::Field, Some("Color")),
            row("label", SymbolKind::Method, Some("Color")),
            row("Perm", SymbolKind::Enum, None),
            row("R", SymbolKind::Field, Some("Perm")),
            row("Size", SymbolKind::Enum, None),
            row("S", SymbolKind::Field, Some("Size")),
            row("Mode", SymbolKind::Enum, None),
            row("A", SymbolKind::Field, Some("Mode")),
            row("Plain", SymbolKind::Class, None),
            row("X", SymbolKind::Field, Some("Plain")),
        ]
    );
}

#[test]
fn a_base_named_enum_from_anywhere_else_proves_nothing() {
    for source in [
        "from .enum import Enum\nclass Color(Enum):\n    RED = 1\n",
        "from models.enum import Enum\nclass Color(Enum):\n    RED = 1\n",
        "from enum import Enum\nEnum = object\nclass Color(Enum):\n    RED = 1\n",
        "import models as enum\nclass Color(enum.Enum):\n    RED = 1\n",
    ] {
        let rows = kinds(source);
        let color = rows.iter().find(|r| r.0 == "Color").unwrap();
        assert_eq!(color.1, SymbolKind::Class, "{source}");
    }
    let local = "class Enum:\n    pass\nclass Color(Enum):\n    RED = 1\n";
    assert_eq!(
        kinds(local),
        [
            row("Enum", SymbolKind::Class, None),
            row("Color", SymbolKind::Class, None),
            row("RED", SymbolKind::Field, Some("Color")),
        ]
    );
}
