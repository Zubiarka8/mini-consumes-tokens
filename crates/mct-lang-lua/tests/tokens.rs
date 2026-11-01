//! Token extraction for Lua: kinds, literals, trivia, callables and errors.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{TokenKind, TokenSource, TokenizedFile};
use mct_lang_lua::LuaTokenSource;
use TokenKind::{Identifier, Keyword, Literal, Punctuation};

fn tokenize(source: &str) -> TokenizedFile {
    LuaTokenSource.tokenize("pkg/mod.lua", source).unwrap()
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
        LuaTokenSource.language_id(),
        mct_lang_lua::LuaParser.language_id()
    );
}

#[test]
fn kinds_are_keywords_identifiers_literals_and_punctuation() {
    let file = tokenize("local x = f(1, \"a\") .. nil\n");
    assert_eq!(
        pairs(&file),
        vec![
            (Keyword, "local"),
            (Identifier, "x"),
            (Punctuation, "="),
            (Identifier, "f"),
            (Punctuation, "("),
            (Literal, "1"),
            (Punctuation, ","),
            (Literal, "\"a\""),
            (Punctuation, ")"),
            (Punctuation, ".."),
            (Literal, "nil"),
        ]
    );
    assert!(file.error_lines.is_empty());
}

#[test]
fn comments_and_long_comments_produce_no_tokens() {
    let plain = tokenize("local x = 1\n");
    let commented = tokenize("-- note\nlocal x = 1 -- tail\n--[[ long\ncomment ]]\n");
    assert_eq!(pairs(&plain), pairs(&commented));
}

#[test]
fn a_long_string_is_one_literal_covering_every_line() {
    let file = tokenize("local s = [[\nline one\nline two\n]]\n");
    let literals: Vec<_> = file
        .tokens
        .iter()
        .filter(|token| token.kind == Literal)
        .collect();
    assert_eq!(literals.len(), 1, "{file:?}");
    assert!(literals[0].text.contains("line two"));
}

#[test]
fn a_shebang_line_is_trivia() {
    let file = tokenize("#!/usr/bin/env lua\nreturn 1\n");
    assert_eq!(pairs(&file), vec![(Keyword, "return"), (Literal, "1")]);
}

#[test]
fn named_and_assigned_functions_are_callables_named_like_their_symbols() {
    let source = "local function a()\n  return 1\nend\n\nfunction M.b()\n  return 2\nend\n\nfunction M:c()\n  return 3\nend\n\nM.d = function()\n  return 4\nend\n";
    let file = tokenize(source);
    let names: Vec<_> = file
        .callables
        .as_ref()
        .unwrap()
        .iter()
        .map(|callable| callable.name.as_str())
        .collect();
    assert_eq!(names, vec!["a", "b", "c", "d"]);
}

#[test]
fn a_syntax_error_is_reported_by_line_not_as_a_failure() {
    let file = tokenize("local x = 1\nlocal = = 2\n");
    assert!(!file.error_lines.is_empty());
}

#[test]
fn deeply_nested_source_does_not_overflow_the_stack() {
    let depth = 5_000;
    let source = format!("local t = {}{}\n", "{".repeat(depth), "}".repeat(depth));
    let _ = LuaTokenSource.tokenize("deep.lua", &source);
}
