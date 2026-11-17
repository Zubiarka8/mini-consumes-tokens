//! A bare-word list of command arguments with a blank after the comma
//! (`Remove-Item a, b -Force`, `Remove-Item Env:A, Env:B`) is a syntax error
//! in the pinned grammar and dropped the whole file. PowerShell accepts it,
//! so the parser reparses with the blank after the comma replaced, at the
//! same offsets, and keeps the tree only when it is clean.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile};
use mct_lang_powershell::PowerShellParser;

fn parse(source: &str) -> Result<mct_core::ParsedFile, mct_core::ParseError> {
    PowerShellParser.parse(&SourceFile {
        relative_path: "a.ps1".to_string(),
        contents: source.to_string(),
    })
}

#[test]
fn a_comma_list_of_bare_words_parses() {
    for source in [
        "Remove-Item a, b -Force\n",
        "Remove-Item Env:A, Env:B\n",
        "Remove-Item Env:MCT_A, Env:MCT_B -ErrorAction SilentlyContinue\n",
        "Remove-Item a, b, c\n",
        "function F {\n    Remove-Item a, b -Force\n    Get-Item x\n}\n",
    ] {
        let parsed = parse(source).unwrap_or_else(|e| panic!("{source:?}: {e:?}"));
        assert!(
            parsed
                .relations
                .iter()
                .any(|r| r.kind == RelationKind::Calls && r.to_name == "Remove-Item"),
            "{source:?}"
        );
    }
}

#[test]
fn a_real_syntax_error_is_still_rejected() {
    assert!(parse("Remove-Item a, b -Force }\n").is_err());
    assert!(parse("function F { Remove-Item a, b\n").is_err());
}
