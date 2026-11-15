//! String literals worth an exact-text search: each run of fixed text of a
//! string, verbatim (`@"…"`), raw (`"""…"""`) or interpolated (`$"…"`)
//! string, between `{…}` holes, that the shared prose filter keeps, on the
//! line it starts on. Escapes are resolved in a plain or interpolated
//! string; a verbatim string keeps its backslashes and reads `""` as one
//! quote; `{{` in an interpolated string is one brace.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::QueryScope;
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"namespace App;

public class Db
{
    public void Connect(string url)
    {
        Log("first line\nsecond \"quoted\" line");
        throw new InvalidOperationException($"Error de conexión con la BD: {url,5:x} (retry {{n}} failed)");
    }

    private const string Path = @"Stored under C:\data with ""quotes"" kept";
    private const string Usage = """
        Usage: tool [flags]
        spans two lines
        """;
    private string Verbatim(int x) => $@"value {x} is ""odd"" today";
}
"#;

#[test]
fn prose_fragments_are_literals() {
    let parsed = mct_corpus::parse_source(&CSharpParser, "App/Db.cs", "C#", SOURCE);
    let literals: Vec<(&str, u32)> = parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect();
    assert_eq!(
        literals,
        [
            ("first line second \"quoted\" line", 7),
            ("Error de conexión con la BD:", 8),
            ("(retry {n} failed)", 8),
            ("Stored under C:\\data with \"quotes\" kept", 11),
            ("Usage: tool [flags] spans two lines", 13),
            ("is \"odd\" today", 16),
        ]
    );
}

#[test]
fn an_exact_phrase_finds_the_literal_and_its_method() {
    let root = std::env::temp_dir().join(format!("mct-cs-literals-{}", std::process::id()));
    std::fs::create_dir_all(root.join("App")).unwrap();
    std::fs::write(root.join("App/Db.cs"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CSharpParser));
    let hits: Vec<String> = index
        .search_literals("conexión con la BD", QueryScope::default())
        .unwrap()
        .into_iter()
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.symbol))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["App/Db.cs:8 Some((\"Connect\", \"method\"))"]);
}
