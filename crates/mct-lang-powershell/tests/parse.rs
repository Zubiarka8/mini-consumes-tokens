// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-powershell/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_powershell::PowerShellParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    PowerShellParser
        .parse(&SourceFile { relative_path: "Deploy.ps1".to_string(), contents: src.to_string() })
        .expect("valid PowerShell source should parse")
}

#[test]
fn extracts_function_and_call() {
    let parsed = parse(
        "function Write-Log($Message) {\n  Write-Output $Message\n}\n\nfunction Invoke-Build {\n  Write-Log \"building\"\n}\n",
    );
    let build = parsed.symbols.iter().find(|s| s.name == "Invoke-Build").unwrap();
    assert_eq!(build.kind, SymbolKind::Function);
    assert_eq!(build.parent.as_deref(), Some("Deploy"), "top-level function's parent is its module (file)");

    let calls: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Calls).map(|r| r.to_name.as_str()).collect();
    assert!(calls.contains(&"Write-Log"));
    assert!(calls.contains(&"Write-Output"), "cmdlet calls are recorded too, as unresolved calls");
}

#[test]
fn top_level_variable_assignment_is_indexed_with_scope_stripped() {
    let parsed = parse("$Version = \"1.0.0\"\n$script:Retries = 3\n");
    let version = parsed.symbols.iter().find(|s| s.name == "Version").unwrap();
    assert_eq!(version.kind, SymbolKind::Variable);
    let retries = parsed.symbols.iter().find(|s| s.name == "Retries").unwrap();
    assert_eq!(retries.kind, SymbolKind::Variable, "script: scope prefix must be stripped from the recorded name");
}

#[test]
fn variable_assigned_inside_a_function_is_not_indexed() {
    let parsed = parse("function Run {\n  $x = 1\n}\n");
    assert!(parsed.symbols.iter().all(|s| s.kind != SymbolKind::Variable), "function-local assignment must not become a top-level Variable symbol");
}

#[test]
fn dot_sourcing_and_import_module_are_imports() {
    let parsed = parse(". .\\lib.ps1\nImport-Module MyModule\n");
    let imports: Vec<_> = parsed.relations.iter().filter(|r| r.kind == RelationKind::Imports).map(|r| r.to_name.as_str()).collect();
    assert!(imports.contains(&".\\lib.ps1"));
    assert!(imports.contains(&"MyModule"));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = PowerShellParser.parse(&SourceFile {
        relative_path: "broken.ps1".to_string(),
        contents: "function Foo( {\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}
