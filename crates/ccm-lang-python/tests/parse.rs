// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/ccm-lang-python/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ccm_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use ccm_lang_python::PythonParser;

fn parse(src: &str) -> ccm_core::ParsedFile {
    PythonParser
        .parse(&SourceFile {
            relative_path: "pkg/mod.py".to_string(),
            contents: src.to_string(),
        })
        .expect("valid Python source should parse")
}

#[test]
fn extracts_function_and_call() {
    let parsed = parse(
        "def helper():\n    return 42\n\ndef main():\n    x = helper()\n    print(x)\n",
    );
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
    let parsed = parse(
        "import functools\n\n@functools.lru_cache\ndef expensive():\n    return 1\n",
    );
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
fn extracts_import_and_from_import() {
    let parsed = parse("import os\nfrom collections import OrderedDict, defaultdict as dd\n");
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"os"));
    assert!(imports.contains(&"OrderedDict"));
    assert!(imports.contains(&"dd"));
}

#[test]
fn function_end_line_spans_the_whole_multiline_body() {
    let parsed = parse("def multiline():\n    x = 1\n    y = 2\n    return x + y\n");
    let sym = parsed
        .symbols
        .iter()
        .find(|s| s.name == "multiline")
        .expect("multiline function should be indexed");
    assert_eq!(sym.location.line, 1);
    assert_eq!(sym.location.end_line, Some(4));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = PythonParser.parse(&SourceFile {
        relative_path: "pkg/broken.py".to_string(),
        contents: "def broken(:\n    ***invalid***\n".to_string(),
    });
    assert!(matches!(result, Err(ccm_core::ParseError::Syntax { .. })));
}
