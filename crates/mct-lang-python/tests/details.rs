//! Declaration details (ADR-034, issue #321) the Python parser proves from
//! syntax: the header as written (no decorators or body), parameters with
//! annotations, defaults and their kind (`*args`, keyword-only, `**kw`), the
//! method receiver, the return annotation, PEP 695 generics, and modifiers
//! from `async` and the standard decorators.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{Modifier, ParsedFile, SymbolDetail};
use mct_lang_python::PythonParser;

const SOURCE: &str = r#"
class Repo(Base, Generic[T], metaclass=Meta):
    @staticmethod
    def make(a, /, b: int = 1, *args: str, key, flag: bool = False, **kw) -> "Repo":
        pass

    async def load[K](
        self, ids: list[K]
    ) -> None: ...

    @classmethod
    @abc.abstractmethod
    def build(cls): ...

def free(x, *, y=2): pass

type Alias[T] = list[T]
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&PythonParser, "pkg/repo.py", "Python", SOURCE)
}

fn detail(parsed: &ParsedFile, name: &str) -> SymbolDetail {
    let symbols: Vec<_> = parsed.symbols.iter().filter(|s| s.name == name).collect();
    assert_eq!(symbols.len(), 1, "{name}: {symbols:?}");
    parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == symbols[0].id)
        .cloned()
        .unwrap_or_else(|| panic!("no detail for {name}"))
        .normalized()
}

/// `name:type=default:kind` per parameter.
fn params(detail: &SymbolDetail) -> Vec<String> {
    detail
        .parameters
        .iter()
        .map(|p| {
            format!(
                "{}:{}={}:{}",
                p.name.as_deref().unwrap_or("_"),
                p.type_text.as_ref().map_or("?", |t| t.text.as_str()),
                p.default.as_deref().unwrap_or("-"),
                p.kind.as_str()
            )
        })
        .collect()
}

#[test]
fn a_static_method_has_no_receiver_and_records_every_parameter_kind() {
    let parsed = parsed();
    let make = detail(&parsed, "make");
    assert_eq!(
        make.signature.as_deref(),
        Some("def make(a, /, b: int = 1, *args: str, key, flag: bool = False, **kw) -> \"Repo\"")
    );
    assert_eq!(make.modifiers, [Modifier::Static]);
    assert_eq!(
        params(&make),
        [
            "a:?=-:positional",
            "b:int=1:positional",
            "args:str=-:variadic",
            "key:?=-:keyword_only",
            "flag:bool=False:keyword_only",
            "kw:?=-:keyword_variadic"
        ]
    );
    assert_eq!(
        make.return_type.map(|t| t.text).as_deref(),
        Some("\"Repo\"")
    );
    // Python has no visibility syntax: `_name` is only a convention.
    assert_eq!(make.visibility, None);
}

#[test]
fn a_method_receiver_generics_async_and_stacked_decorators() {
    let parsed = parsed();
    let load = detail(&parsed, "load");
    assert_eq!(
        load.signature.as_deref(),
        Some("async def load[K]( self, ids: list[K] ) -> None")
    );
    assert_eq!(load.modifiers, [Modifier::Async]);
    assert_eq!(load.generics.as_deref(), Some("[K]"));
    assert_eq!(
        params(&load),
        ["self:?=-:receiver", "ids:list[K]=-:positional"]
    );
    let build = detail(&parsed, "build");
    assert_eq!(build.modifiers, [Modifier::Abstract]);
    assert_eq!(params(&build), ["cls:?=-:receiver"]);
}

#[test]
fn a_bare_star_makes_the_rest_keyword_only_and_classes_get_their_header() {
    let parsed = parsed();
    assert_eq!(
        params(&detail(&parsed, "free")),
        ["x:?=-:positional", "y:?=2:keyword_only"]
    );
    let repo = detail(&parsed, "Repo");
    assert_eq!(
        repo.signature.as_deref(),
        Some("class Repo(Base, Generic[T], metaclass=Meta)")
    );
    assert!(!repo.parameters_known);
    let alias = detail(&parsed, "Alias");
    assert_eq!(alias.signature.as_deref(), Some("type Alias[T] = list[T]"));
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-python-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("pkg")).unwrap();
    std::fs::write(root.join("pkg/repo.py"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PythonParser));
    let make = index
        .symbol_details("pkg/repo.py", 4, "make")
        .unwrap()
        .unwrap();
    assert_eq!(make.modifiers, ["static"]);
    assert_eq!(make.parameters.len(), 6);
    std::fs::remove_dir_all(root).unwrap();
}
