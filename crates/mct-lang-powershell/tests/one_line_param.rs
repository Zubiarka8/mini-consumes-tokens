//! The pinned grammar reads `param(...)` followed by a statement on the same
//! line (`function F { param($a) $a + 1 }`) as a syntax error, and the whole
//! file was dropped. PowerShell accepts it, so the parser reparses with a
//! `;` in the blank after the parameter block, at the same offsets, and
//! keeps the result only when it is clean.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_powershell::PowerShellParser;

fn parse(source: &str) -> Result<mct_core::ParsedFile, mct_core::ParseError> {
    PowerShellParser.parse(&SourceFile {
        relative_path: "a.ps1".to_string(),
        contents: source.to_string(),
    })
}

#[test]
fn a_statement_after_a_param_block_on_the_same_line_parses() {
    let parsed = parse(
        "function One { param($a) Write-Host $a }\nfunction Two { param([int]$n)\t$n + 1 }\nfunction Three { param($x) One -a $x }\n",
    )
    .unwrap();
    let functions: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Function)
        .map(|s| (s.name.as_str(), s.location.line))
        .collect();
    assert_eq!(functions, [("One", 1), ("Two", 2), ("Three", 3)]);
    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| {
            (
                parsed
                    .symbols
                    .iter()
                    .find(|s| s.id == r.from)
                    .unwrap()
                    .name
                    .as_str(),
                r.to_name.as_str(),
            )
        })
        .collect();
    assert!(calls.contains(&("One", "Write-Host")), "{calls:?}");
    assert!(calls.contains(&("Three", "One")), "{calls:?}");
}

#[test]
fn a_real_syntax_error_is_still_rejected() {
    assert!(parse("function One { param($a) $a + }\n").is_err());
    assert!(parse("function One { param($a\n").is_err());
}
