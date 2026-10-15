//! Token extraction for Rust (issue #207): kinds, literals, trivia, lines,
//! syntax errors and pathological nesting.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{TokenKind, TokenSource, TokenizedFile};
use mct_lang_rust::RustTokenSource;
use TokenKind::{Identifier, Keyword, Literal, Punctuation};

fn tokenize(source: &str) -> TokenizedFile {
    RustTokenSource.tokenize("src/lib.rs", source).unwrap()
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
        RustTokenSource.language_id(),
        mct_lang_rust::RustParser.language_id()
    );
}

#[test]
fn keywords_are_told_apart_from_identifiers() {
    let file = tokenize("pub fn add(self, x: i32) -> i32 { if x > 0 { return x; } x }\n");
    assert!(file.error_lines.is_empty());
    let tokens = pairs(&file);
    for keyword in ["pub", "fn", "self", "if", "return"] {
        assert!(tokens.contains(&(Keyword, keyword)), "{keyword}");
    }
    assert!(tokens.contains(&(Identifier, "add")));
    assert!(tokens.contains(&(Identifier, "x")));
    // A primitive type is a name, not a keyword.
    assert!(tokens.contains(&(Identifier, "i32")));
    assert!(!tokens.contains(&(Keyword, "add")));
}

#[test]
fn operators_and_punctuation_are_kept() {
    let file = tokenize("fn f() -> u8 { a::b(1) + 2 * c.d }\n");
    let punctuation: Vec<&str> = file
        .tokens
        .iter()
        .filter(|token| token.kind == Punctuation)
        .map(|token| token.text.as_str())
        .collect();
    assert_eq!(
        punctuation,
        ["(", ")", "->", "{", "::", "(", ")", "+", "*", ".", "}"]
    );
}

#[test]
fn every_kind_of_literal_is_a_single_token() {
    let source = concat!(
        "fn f() {\n",
        "    let _ = (1_000u32, 0xFF, 1.5e3f64, true, 'x', '\\n', b'a');\n",
        "    let _ = (\"es\\n\\\"c\", r#\"raw \"q\"\"#, b\"by\", br\"x\", c\"hi\");\n",
        "}\n"
    );
    let file = tokenize(source);
    let literals: Vec<&str> = file
        .tokens
        .iter()
        .filter(|token| token.kind == Literal)
        .map(|token| token.text.as_str())
        .collect();
    assert_eq!(
        literals,
        [
            "1_000u32",
            "0xFF",
            "1.5e3f64",
            "true",
            "'x'",
            "'\\n'",
            "b'a'",
            "\"es\\n\\\"c\"",
            "r#\"raw \"q\"\"#",
            "b\"by\"",
            "br\"x\"",
            "c\"hi\"",
        ]
    );
}

#[test]
fn a_multi_line_string_is_one_token_spanning_its_lines() {
    let source = "fn f() {\n    let s = \"a\nb\nc\";\n    let n = 1;\n}\n";
    let file = tokenize(source);
    let string = file
        .tokens
        .iter()
        .find(|token| token.kind == Literal && token.text.starts_with('"'))
        .unwrap();
    assert_eq!((string.line, string.end_line), (2, 4));
    assert_eq!(&source[string.start_byte..string.end_byte], string.text);
    let one = file.tokens.iter().find(|token| token.text == "1").unwrap();
    assert_eq!((one.line, one.end_line), (5, 5));
}

#[test]
fn comments_doc_comments_and_whitespace_produce_no_tokens() {
    let source = "#!/usr/bin/env run\n//! crate doc\n/// outer doc\n/* block\n   spanning */\nfn f() {} // trailing\n";
    let file = tokenize(source);
    assert!(file.error_lines.is_empty());
    assert_eq!(
        pairs(&file),
        [
            (Keyword, "fn"),
            (Identifier, "f"),
            (Punctuation, "("),
            (Punctuation, ")"),
            (Punctuation, "{"),
            (Punctuation, "}"),
        ]
    );
    // Comment-like text inside a string is part of the literal.
    let file = tokenize("fn f() { let s = \"// not a comment\"; }\n");
    assert!(file
        .tokens
        .iter()
        .any(|token| token.kind == Literal && token.text == "\"// not a comment\""));
}

#[test]
fn lifetimes_labels_macros_and_paths_are_tokenized() {
    let source = "macro_rules! m { ($x:expr) => { $x }; }\nfn f<'a>(s: &'a str) { 'o: loop { println!(\"{}\", s); break 'o; } }\nuse crate::a::{self, super::b};\n";
    let file = tokenize(source);
    assert!(file.error_lines.is_empty());
    let tokens = pairs(&file);
    assert!(tokens.contains(&(Keyword, "macro_rules!")));
    assert!(tokens.contains(&(Identifier, "$x")));
    assert!(tokens.contains(&(Identifier, "println")));
    assert!(tokens.contains(&(Literal, "\"{}\"")));
    assert!(tokens.contains(&(Keyword, "crate")));
    assert!(tokens.contains(&(Keyword, "super")));
    assert!(tokens.contains(&(Keyword, "loop")));
    assert!(tokens.contains(&(Identifier, "o")));
}

#[test]
fn token_offsets_are_in_order_and_index_the_source() {
    let source = "fn \u{e9}t\u{e9}() { let s = \"\u{1f980}\"; }\n";
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
    let source = "fn ok() {}\nfn broken( {\nfn also_ok() {}\n";
    let file = tokenize(source);
    assert!(
        file.error_lines
            .iter()
            .any(|range| range.start <= 2 && range.end >= 2),
        "{:?}",
        file.error_lines
    );
    assert!(file.tokens.iter().any(|token| token.text == "ok"));
    assert!(!tokenize("fn ok() {}\n").tokens.is_empty());
    assert!(tokenize("fn ok() {}\n").error_lines.is_empty());
}

#[test]
fn garbage_and_empty_input_never_panic() {
    assert!(tokenize("").tokens.is_empty());
    let file = tokenize("}}}} \u{0} ;;; @@@ \"unterminated\n");
    assert!(!file.error_lines.is_empty());
}

#[test]
fn pathologically_deep_nesting_does_not_overflow_the_stack() {
    let depth = 10_000;
    let source = format!(
        "fn f() {{ let _ = {}1{}; }}\n",
        "(".repeat(depth),
        ")".repeat(depth)
    );
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(move || tokenize(&source).tokens.len())
        .unwrap();
    assert!(handle.join().unwrap() > 2 * depth);
}
