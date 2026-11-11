//! Declaration details (ADR-034, issue #321) the C++ parser proves from
//! syntax: the header without attributes or body (with its `template <…>`),
//! parameters (a type is everything but the name: `const char*`, `int&`;
//! `...` is variadic), the return type (a trailing `-> T` included),
//! visibility from the access label in force (or the class/struct default)
//! and modifiers (`= 0` is abstract).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{Modifier, ParsedFile, SymbolDetail, SymbolKind, Visibility};
use mct_lang_cpp::CppParser;

const SOURCE: &str = r#"
namespace app {
class Repo : public Base {
public:
    explicit Repo(int x) : base_(x) {}
    virtual ~Repo() = default;
    template <typename K>
    static std::vector<K> load(const char* fmt, int id = 3, ...) const;
    virtual void run(int& a) override final;
    virtual int size() const = 0;
protected:
    static constexpr int limit = 3, max = 9;
private:
    std::string name_;
};
struct Point { int x; };
[[nodiscard]] inline int* top(const std::string& s) { return nullptr; }
int Repo::size() const { return 1; }
auto later(int a) -> long { return a; }
extern int counter;
using Id = long;
typedef unsigned Count, *CountPtr;
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CppParser, "src/repo.cpp", "C++", SOURCE)
}

fn detail(parsed: &ParsedFile, name: &str, line: u32) -> SymbolDetail {
    let symbol = parsed
        .symbols
        .iter()
        .find(|s| s.name == name && s.location.line == line && s.kind != SymbolKind::Module)
        .unwrap_or_else(|| panic!("no {name} on line {line}"));
    parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == symbol.id)
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

fn returns(detail: &SymbolDetail) -> Option<&str> {
    detail.return_type.as_ref().map(|t| t.text.as_str())
}

#[test]
fn a_member_template_keeps_its_template_header_and_c_varargs() {
    let parsed = parsed();
    let load = detail(&parsed, "load", 8);
    assert_eq!(
        load.signature.as_deref(),
        Some(
            "template <typename K> static std::vector<K> load(const char* fmt, int id = 3, \
             ...) const"
        )
    );
    assert_eq!(load.visibility, Some(Visibility::Public));
    assert_eq!(load.modifiers, [Modifier::Static]);
    assert_eq!(load.generics.as_deref(), Some("<typename K>"));
    assert_eq!(returns(&load), Some("std::vector<K>"));
    assert_eq!(
        params(&load),
        [
            "fmt:const char*=-:positional",
            "id:int=3:positional",
            "_:?=-:variadic"
        ]
    );
}

#[test]
fn virtual_specifiers_and_pure_virtual_are_modifiers() {
    let parsed = parsed();
    let run = detail(&parsed, "run", 9);
    assert_eq!(
        run.signature.as_deref(),
        Some("virtual void run(int& a) override final")
    );
    assert_eq!(
        run.modifiers,
        [Modifier::Final, Modifier::Override, Modifier::Virtual]
    );
    assert_eq!(params(&run), ["a:int&=-:positional"]);
    assert_eq!(returns(&run), Some("void"));
    let size = detail(&parsed, "size", 10);
    assert_eq!(size.modifiers, [Modifier::Abstract, Modifier::Virtual]);
    let destructor = detail(&parsed, "~Repo", 6);
    assert_eq!(destructor.signature.as_deref(), Some("virtual ~Repo()"));
    let constructor = detail(&parsed, "Repo", 5);
    assert_eq!(
        constructor.signature.as_deref(),
        Some("explicit Repo(int x)")
    );
    assert!(constructor.return_type.is_none());
}

#[test]
fn visibility_is_the_access_label_in_force_or_the_class_key_default() {
    let parsed = parsed();
    for (name, signature) in [
        ("limit", "static constexpr int limit"),
        ("max", "static constexpr int max"),
    ] {
        let field = detail(&parsed, name, 12);
        assert_eq!(field.signature.as_deref(), Some(signature));
        assert_eq!(field.visibility, Some(Visibility::Protected));
        assert_eq!(field.modifiers, [Modifier::Const, Modifier::Static]);
    }
    let name = detail(&parsed, "name_", 14);
    assert_eq!(name.signature.as_deref(), Some("std::string name_"));
    assert_eq!(name.visibility, Some(Visibility::Private));
    assert_eq!(
        detail(&parsed, "x", 16).visibility,
        Some(Visibility::Public)
    );
    // Outside a class, C++ has linkage, not access: none is recorded, and
    // an out-of-line definition's access is its declaration's.
    let repo = detail(&parsed, "Repo", 3);
    assert_eq!(repo.signature.as_deref(), Some("class Repo : public Base"));
    assert_eq!(repo.visibility, None);
    let out_of_line = detail(&parsed, "size", 18);
    assert_eq!(
        out_of_line.signature.as_deref(),
        Some("int Repo::size() const")
    );
    assert_eq!(out_of_line.visibility, None);
}

#[test]
fn free_functions_and_variables_get_their_header_and_return_type() {
    let parsed = parsed();
    let top = detail(&parsed, "top", 17);
    assert_eq!(
        top.signature.as_deref(),
        Some("inline int* top(const std::string& s)")
    );
    assert_eq!(top.modifiers, [Modifier::Inline]);
    assert_eq!(returns(&top), Some("int*"));
    assert_eq!(params(&top), ["s:const std::string&=-:positional"]);
    assert_eq!(returns(&detail(&parsed, "later", 19)), Some("long"));
    let counter = detail(&parsed, "counter", 20);
    assert_eq!(counter.signature.as_deref(), Some("extern int counter"));
    assert_eq!(counter.modifiers, [Modifier::Extern]);
}

#[test]
fn a_type_alias_is_its_whole_declaration() {
    let parsed = parsed();
    assert_eq!(
        detail(&parsed, "Id", 21).signature.as_deref(),
        Some("using Id = long")
    );
    assert_eq!(
        detail(&parsed, "CountPtr", 22).signature.as_deref(),
        Some("typedef unsigned Count, *CountPtr")
    );
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-cpp-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/repo.cpp"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CppParser));
    let run = index
        .symbol_details("src/repo.cpp", 9, "run")
        .unwrap()
        .unwrap();
    assert_eq!(run.modifiers, ["final", "override", "virtual"]);
    assert_eq!(run.parameters.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
