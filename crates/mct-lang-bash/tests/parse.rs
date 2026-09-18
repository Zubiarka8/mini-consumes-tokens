// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-bash/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_bash::BashParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    BashParser
        .parse(&SourceFile { relative_path: "deploy.sh".to_string(), contents: src.to_string() })
        .expect("valid bash source should parse")
}

#[test]
fn extracts_function_and_call() {
    let parsed = parse("helper() {\n  log \"called\"\n}\n\ngreet() {\n  helper\n  echo \"hi\"\n}\n");
    let greet = parsed.symbols.iter().find(|s| s.name == "greet").unwrap();
    assert_eq!(greet.kind, SymbolKind::Function);
    assert_eq!(greet.parent.as_deref(), Some("deploy"), "top-level function's parent is its module (file)");

    let calls: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Calls).map(|r| r.to_name.as_str()).collect();
    assert!(calls.contains(&"helper"));
    assert!(calls.contains(&"echo"), "external commands are recorded too, as unresolved calls");
}

#[test]
fn top_level_variable_assignment_is_indexed() {
    let parsed = parse("FOO=bar\nexport BAZ=\"qux\"\n");
    let foo = parsed.symbols.iter().find(|s| s.name == "FOO").unwrap();
    assert_eq!(foo.kind, SymbolKind::Variable);
    let baz = parsed.symbols.iter().find(|s| s.name == "BAZ").unwrap();
    assert_eq!(baz.kind, SymbolKind::Variable, "export-wrapped assignment is still indexed");
}

#[test]
fn variable_assigned_inside_a_function_is_not_indexed() {
    let parsed = parse("run() {\n  local x=1\n}\n");
    assert!(parsed.symbols.iter().all(|s| s.kind != SymbolKind::Variable), "function-local assignment must not become a top-level Variable symbol");
}

#[test]
fn source_with_literal_path_is_an_import() {
    let parsed = parse("source ./lib.sh\n. \"$HOME/.profile\"\n");
    let imports: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Imports).map(|r| r.to_name.as_str()).collect();
    assert!(imports.contains(&"./lib.sh"));
    assert!(imports.contains(&"$HOME/.profile"));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = BashParser.parse(&SourceFile {
        relative_path: "broken.sh".to_string(),
        contents: "function foo( {\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}
