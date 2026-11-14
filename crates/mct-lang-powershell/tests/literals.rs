//! String literals worth an exact-text search: each run of fixed text of an
//! expandable string or here-string (`"…"`, `@"…"@`), between `$x` /
//! `$(…)` / `${…}` interpolations, and each verbatim string or here-string
//! (`'…'`, `@'…'@`), that the shared prose filter keeps, on the line it
//! starts on. A backtick escape is resolved (`` `" `` is a quote, `` `t ``
//! a space) and a doubled quote is one; a backslash is plain text.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::QueryScope;
use mct_lang_powershell::PowerShellParser;

const SOURCE: &str = r#"function Connect-Db($url) {
    Write-Host "first line `"quoted`" and C:\data kept"
    throw "Error de conexión con la BD: $url (retry $($n + 1) at ${env:HOME} failed)"
}
Write-Host 'it''s stored under C:\path here'
$usage = @"
  Usage: tool $name
  spans two lines
"@
$raw = @'
  Verbatim text keeps $name as written
'@
"#;

#[test]
fn prose_fragments_are_literals() {
    let parsed = mct_corpus::parse_source(&PowerShellParser, "Db.ps1", "PowerShell", SOURCE);
    let literals: Vec<(&str, u32)> = parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect();
    assert_eq!(
        literals,
        [
            ("first line \"quoted\" and C:\\data kept", 2),
            ("Error de conexión con la BD:", 3),
            ("it's stored under C:\\path here", 5),
            ("Usage: tool", 7),
            ("spans two lines", 8),
            ("Verbatim text keeps $name as written", 11),
        ]
    );
}

#[test]
fn an_exact_phrase_finds_the_literal_and_its_function() {
    let root = std::env::temp_dir().join(format!("mct-ps-literals-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Db.ps1"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PowerShellParser));
    let hits: Vec<String> = index
        .search_literals("conexión con la BD", QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.symbol))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["Db.ps1:3 Some((\"Connect-Db\", \"function\"))"]);
}
