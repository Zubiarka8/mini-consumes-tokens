//! `JsTsTokenSource` on its own: token kinds, roles and callable spans.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LiteralCategory, TokenKind, TokenRole, TokenSource, TokenizedFile};
use mct_lang_js_ts::{JsTsParser, JsTsTokenSource};

fn tokenize(path: &str, source: &str) -> TokenizedFile {
    JsTsTokenSource.tokenize(path, source).unwrap()
}

fn text<'a>(source: &'a str, file: &TokenizedFile) -> Vec<&'a str> {
    file.tokens
        .iter()
        .map(|t| &source[t.start_byte..t.end_byte])
        .collect()
}

#[test]
fn language_id_matches_the_parser() {
    use mct_core::LanguageParser;
    assert_eq!(JsTsTokenSource.language_id(), JsTsParser.language_id());
}

#[test]
fn comments_produce_no_tokens_and_a_template_is_one_literal() {
    let source = "// note\nconst a = `x ${y} z`; /* block */\n";
    let file = tokenize("a.js", source);
    assert_eq!(text(source, &file), ["const", "a", "=", "`x ${y} z`", ";"]);
    assert_eq!(
        file.tokens[3].role,
        TokenRole::Literal(LiteralCategory::InterpolatedString)
    );
    assert_eq!(file.tokens[3].kind, TokenKind::Literal);
}

#[test]
fn roles_follow_the_syntax() {
    let source = "function f(a: T) { g(a.b); }\n";
    let file = tokenize("a.ts", source);
    let role = |word: &str| {
        let i = text(source, &file).iter().position(|t| *t == word).unwrap();
        file.tokens[i].role
    };
    assert_eq!(role("f"), TokenRole::Declaration);
    assert_eq!(role("a"), TokenRole::Parameter);
    assert_eq!(role("T"), TokenRole::Type);
    assert_eq!(role("g"), TokenRole::Callee);
    assert_eq!(role("b"), TokenRole::Member);
}

#[test]
fn a_javascript_rest_parameter_is_a_parameter() {
    let source = "function f(...rest) {}\n";
    let file = tokenize("a.js", source);
    let i = text(source, &file)
        .iter()
        .position(|t| *t == "rest")
        .unwrap();
    assert_eq!(file.tokens[i].role, TokenRole::Parameter);
}

#[test]
fn callables_span_what_the_parser_indexes() {
    let source =
        "class C {\n  m() {}\n  f = () => 1;\n}\nconst g = function () {};\nexports.h = () => 2;\n";
    let file = tokenize("a.ts", source);
    let callables = file.callables.unwrap();
    let spans: Vec<(&str, &str)> = callables
        .iter()
        .map(|c| (c.name.as_str(), &source[c.bytes.clone()]))
        .collect();
    assert_eq!(
        spans,
        [
            ("m", "m() {}"),
            ("f", "f = () => 1"),
            ("g", "g = function () {}"),
            ("h", "() => 2"),
        ]
    );
}
