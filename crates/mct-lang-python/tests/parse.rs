// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-python/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_python::PythonParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    mct_corpus::parse_source(&PythonParser, "pkg/mod.py", "Python", src)
}

#[test]
fn extracts_function_and_call() {
    let parsed =
        parse("def helper():\n    return 42\n\ndef main():\n    x = helper()\n    print(x)\n");
    let names: Vec<_> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"helper"));
    assert!(names.contains(&"main"));

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"helper"));
    assert!(calls.contains(&"print"));
}

#[test]
fn extracts_class_with_base_and_methods() {
    let parsed = parse(
        "class Animal:\n    pass\n\nclass Dog(Animal):\n    def bark(self):\n        return 'woof'\n",
    );
    let class_sym = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Dog")
        .expect("Dog class should be indexed");
    assert_eq!(class_sym.kind, SymbolKind::Class);

    let method = parsed
        .symbols
        .iter()
        .find(|s| s.name == "bark")
        .expect("bark method should be indexed");
    assert_eq!(method.kind, SymbolKind::Method);
    assert_eq!(method.parent.as_deref(), Some("Dog"));

    let extends: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Extends)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(extends.contains(&"Animal"));
}

#[test]
fn decorated_function_is_still_indexed_with_decorator_recorded() {
    let parsed =
        parse("import functools\n\n@functools.lru_cache\ndef expensive():\n    return 1\n");
    let func = parsed
        .symbols
        .iter()
        .find(|s| s.name == "expensive")
        .expect("decorated function should still be indexed");
    assert_eq!(func.kind, SymbolKind::Function);

    let decorator_refs: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(decorator_refs.contains(&"lru_cache"));
}

#[test]
fn decorator_relation_is_owned_by_the_decorated_function_not_the_module() {
    assert_decorator_owner(
        "@app.route(\"/users\")\ndef list_users():\n    return []\n",
        "list_users",
        "route",
    );
}

#[test]
fn decorator_relation_is_owned_by_the_decorated_class_not_the_module() {
    assert_decorator_owner(
        "@register\nclass Handler:\n    pass\n",
        "Handler",
        "register",
    );
}

fn assert_decorator_owner(source: &str, owner: &str, decorator: &str) {
    let parsed = parse(source);
    let symbol = parsed
        .symbols
        .iter()
        .find(|s| s.name == owner)
        .expect("decorated symbol should still be indexed");

    let decorator_ref = parsed
        .relations
        .iter()
        .find(|r| r.kind == RelationKind::References && r.to_name == decorator)
        .expect("decorator should produce a References relation");
    assert_eq!(
        decorator_ref.from, symbol.id,
        "the decorator relation must be owned by {owner}, not the module"
    );
}

#[test]
fn extracts_import_and_from_import() {
    let parsed = parse("import os\nfrom collections import OrderedDict, defaultdict as dd\n");
    mct_corpus::assert_imports(&parsed, &["os", "OrderedDict", "dd"]);
}

#[test]
fn function_end_line_spans_the_whole_multiline_body() {
    let parsed = parse("def multiline():\n    x = 1\n    y = 2\n    return x + y\n");
    mct_corpus::assert_symbol_span(&parsed, "multiline", 1, 4);
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    mct_corpus::assert_syntax_error_reported(
        &PythonParser,
        "pkg/broken.py",
        "def broken(:\n    ***invalid***\n",
    );
}

#[test]
fn jsx_templates_escape_literal_braces_in_f_strings() {
    let parsed = parse(
        r#"def handlers(event, handler_name):
    return [
        f'{event}={{(e) => console.log("{handler_name}", e)}}',
        f'on{event.capitalize()}={{(e) => console.log("{event}", e)}}',
        f'{{/* Lighting */}} <mesh name="{handler_name}" />',
    ]
"#,
    );
    assert!(parsed
        .symbols
        .iter()
        .any(|symbol| symbol.name == "handlers"));

    let invalid = PythonParser.parse(&SourceFile {
        relative_path: "pkg/broken_handler.py".to_string(),
        contents: r#"def handler(event, handler_name):
    return f'{event}={(e) => console.log("{handler_name}", e)}'
"#
        .to_string(),
    });
    assert!(matches!(invalid, Err(mct_core::ParseError::Syntax { .. })));
}
