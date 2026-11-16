//! D9: a call of a built-in cmdlet (`Write-Host`, `Join-Path`,
//! `ForEach-Object`) the file does not define as a function can only be
//! PowerShell's, so it is external, never a same-named function of another
//! script. PowerShell names ignore case. A function the file defines under
//! the cmdlet's name is that file's own, and a function of the repository
//! (`Get-Lines`) or an external program (`git`) is left alone.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_powershell::PowerShellParser;

const PLAIN: &str = r#"function Run {
    Write-Host "start"
    $p = join-path $PSScriptRoot "x"
    Get-Lines $p | ForEach-Object { $_ }
    git status
}
"#;

const SHADOWED: &str = r#"function Write-Host {
    param($m)
    $m
}
function Run {
    Write-Host "hi"
    Test-Path "x"
}
"#;

fn parsed(source: &str) -> ParsedFile {
    mct_corpus::parse_source(&PowerShellParser, "scripts/a.ps1", "PowerShell", source)
}

/// Every call as `name` plus ` external` when the parser proved it so.
fn calls(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .map(|(i, r)| {
            let external = parsed
                .relation_targets
                .iter()
                .any(|t| t.relation == i && t.external);
            format!("{}{}", r.to_name, if external { " external" } else { "" })
        })
        .collect()
}

#[test]
fn a_builtin_cmdlet_is_external() {
    assert_eq!(
        calls(&parsed(PLAIN)),
        [
            "Write-Host external",
            "join-path external",
            "Get-Lines",
            "ForEach-Object external",
            "git",
        ]
    );
}

#[test]
fn a_cmdlet_the_file_defines_as_a_function_is_not_proven() {
    assert_eq!(
        calls(&parsed(SHADOWED)),
        ["Write-Host", "Test-Path external"]
    );
}
