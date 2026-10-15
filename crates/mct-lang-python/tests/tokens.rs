//! Token extraction for Python (issue #207): kinds, literals, trivia, lines,
//! syntax errors and pathological nesting.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{TokenKind, TokenSource, TokenizedFile};
use mct_lang_python::PythonTokenSource;
use TokenKind::{Identifier, Keyword, Literal, Punctuation};

fn tokenize(source: &str) -> TokenizedFile {
    PythonTokenSource.tokenize("pkg/mod.py", source).unwrap()
}

fn pairs(file: &TokenizedFile) -> Vec<(TokenKind, &str)> {
    file.tokens
        .iter()
        .map(|token| (token.kind, token.text.as_str()))
        .collect()
}

#[test]
fn language_id_matches_the_parser() {
    use mct_core::LanguageParser;
    assert_eq!(
        PythonTokenSource.language_id(),
        mct_lang_python::PythonParser.language_id()
    );
}

#[test]
fn keywords_are_told_apart_from_identifiers() {
    let source =
        "async def add(self, x):\n    if x and not self.done:\n        return await x\n    pass\n";
    let file = tokenize(source);
    assert!(file.error_lines.is_empty());
    let tokens = pairs(&file);
    for keyword in [
        "async", "def", "if", "and", "not", "return", "await", "pass",
    ] {
        assert!(tokens.contains(&(Keyword, keyword)), "{keyword}");
    }
    // `self` is an ordinary name in Python.
    for name in ["add", "self", "x", "done"] {
        assert!(tokens.contains(&(Identifier, name)), "{name}");
    }
    assert!(!tokens.contains(&(Keyword, "add")));
}

#[test]
fn operators_and_punctuation_are_kept() {
    let file = tokenize("def f(a, *b, **c) -> int:\n    return a[0] ** 2 // 3\n");
    let punctuation: Vec<&str> = file
        .tokens
        .iter()
        .filter(|token| token.kind == Punctuation)
        .map(|token| token.text.as_str())
        .collect();
    assert_eq!(
        punctuation,
        ["(", ",", "*", ",", "**", ")", "->", ":", "[", "]", "**", "//"]
    );
}

#[test]
fn every_kind_of_literal_is_a_single_token() {
    let source = concat!(
        "x = [1_000, 0x1F, 1.5e3, 2j, True, False, None, ...]\n",
        "y = ('cc', \"es\\n\\\"c\", r'raw\\d', b\"by\", f\"hi {a!r:>{w}} {{lit}}\")\n",
    );
    let file = tokenize(source);
    assert!(file.error_lines.is_empty());
    let literals: Vec<&str> = file
        .tokens
        .iter()
        .filter(|token| token.kind == Literal)
        .map(|token| token.text.as_str())
        .collect();
    assert_eq!(
        literals,
        [
            "1_000",
            "0x1F",
            "1.5e3",
            "2j",
            "True",
            "False",
            "None",
            "...",
            "'cc'",
            "\"es\\n\\\"c\"",
            "r'raw\\d'",
            "b\"by\"",
            "f\"hi {a!r:>{w}} {{lit}}\"",
        ]
    );
    // The interpolation's names are inside the f-string literal, not tokens.
    assert!(!file.tokens.iter().any(|token| token.text == "w"));
}

#[test]
fn implicit_concatenation_keeps_one_literal_per_string() {
    let file = tokenize("x = 'a' 'b'\n");
    assert_eq!(
        pairs(&file),
        [
            (Identifier, "x"),
            (Punctuation, "="),
            (Literal, "'a'"),
            (Literal, "'b'")
        ]
    );
}

#[test]
fn a_multi_line_string_is_one_token_spanning_its_lines() {
    let source = "def f():\n    \"\"\"doc\n    string\"\"\"\n    return 1\n";
    let file = tokenize(source);
    let doc = file
        .tokens
        .iter()
        .find(|token| token.kind == Literal && token.text.starts_with("\"\"\""))
        .unwrap();
    assert_eq!((doc.line, doc.end_line), (2, 3));
    assert_eq!(&source[doc.start_byte..doc.end_byte], doc.text);
    let one = file.tokens.iter().find(|token| token.text == "1").unwrap();
    assert_eq!((one.line, one.end_line), (4, 4));
}

#[test]
fn comments_line_continuations_and_whitespace_produce_no_tokens() {
    let source = "#!/usr/bin/env python\n# note\nx = (1 +  # why\n     2) \\\n    + 3\n\n\n";
    let file = tokenize(source);
    assert!(file.error_lines.is_empty());
    assert_eq!(
        pairs(&file),
        [
            (Identifier, "x"),
            (Punctuation, "="),
            (Punctuation, "("),
            (Literal, "1"),
            (Punctuation, "+"),
            (Literal, "2"),
            (Punctuation, ")"),
            (Punctuation, "+"),
            (Literal, "3"),
        ]
    );
    // `#` inside a string is part of the literal.
    let file = tokenize("s = '# not a comment'\n");
    assert!(file
        .tokens
        .iter()
        .any(|token| token.kind == Literal && token.text == "'# not a comment'"));
}

#[test]
fn indentation_width_does_not_change_the_tokens() {
    let four = tokenize("if a:\n    b()\n");
    let eight = tokenize("if a:\n        b()\n");
    assert_eq!(pairs(&four), pairs(&eight));
}

#[test]
fn token_offsets_are_in_order_and_index_the_source() {
    let source = "def \u{e9}t\u{e9}():\n    s = '\u{1f980}'\n";
    let file = tokenize(source);
    for token in &file.tokens {
        assert_eq!(&source[token.start_byte..token.end_byte], token.text);
    }
    assert!(file
        .tokens
        .windows(2)
        .all(|pair| pair[0].end_byte <= pair[1].start_byte));
    assert!(file
        .tokens
        .iter()
        .any(|token| token.kind == Identifier && token.text == "\u{e9}t\u{e9}"));
}

#[test]
fn a_syntax_error_is_reported_by_line_without_panicking() {
    let source = "def ok():\n    pass\n\ndef broken(:\n    pass\n\ndef also_ok():\n    pass\n";
    let file = tokenize(source);
    assert!(
        file.error_lines
            .iter()
            .any(|range| range.start <= 4 && range.end >= 4),
        "{:?}",
        file.error_lines
    );
    assert!(file.tokens.iter().any(|token| token.text == "ok"));
    assert!(tokenize("def ok():\n    pass\n").error_lines.is_empty());
}

#[test]
fn garbage_and_empty_input_never_panic() {
    assert!(tokenize("").tokens.is_empty());
    let file = tokenize("))) \u{0} $$ ?? \"unterminated\n  if\n");
    assert!(!file.error_lines.is_empty());
}

#[test]
fn pathologically_deep_nesting_does_not_overflow_the_stack() {
    let depth = 10_000;
    let source = format!("x = {}1{}\n", "(".repeat(depth), ")".repeat(depth));
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(move || tokenize(&source).tokens.len())
        .unwrap();
    assert!(handle.join().unwrap() > 2 * depth);
}
