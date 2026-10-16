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
    for keyword in ["fn", "self", "if", "return"] {
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
fn visibility_produces_no_tokens() {
    // A visibility decides who may use an item, not what it does: the same
    // function under any visibility is the same token sequence.
    let plain = tokenize("fn f(x: u8) -> u8 { x }\n");
    for source in [
        "pub fn f(x: u8) -> u8 { x }\n",
        "pub(crate) fn f(x: u8) -> u8 { x }\n",
        "pub(super) fn f(x: u8) -> u8 { x }\n",
        "pub(in crate::a) fn f(x: u8) -> u8 { x }\n",
    ] {
        let file = tokenize(source);
        assert!(file.error_lines.is_empty(), "{source}");
        assert_eq!(pairs(&file), pairs(&plain), "{source}");
    }
    // Inside a body as well: a nested item's or a field's visibility.
    let file = tokenize("fn f() { struct S { pub a: u8 } pub fn g() {} }\n");
    assert!(file.error_lines.is_empty());
    assert!(!pairs(&file).contains(&(Keyword, "pub")));
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

#[test]
fn identifier_roles_and_literal_categories_are_syntactic_positions() {
    use mct_core::LiteralCategory::{Boolean, Character, Number, String};
    use mct_core::TokenRole::{
        Callee, Declaration, Literal as Lit, Member, Parameter, Type, Unclassified,
    };
    let source = "\
pub fn load(order: &Order, scale: i64) -> Vec<u8> {
    let total = order.total.saturating_mul(scale);
    let Point { x, .. } = audit(total);
    println!(\"{}\", x);
    let f = |item| item;
    helpers::emit('c', true, 1.5)
}
";
    let file = tokenize(source);
    let roles: Vec<(&str, mct_core::TokenRole)> = file
        .tokens
        .iter()
        .filter(|token| matches!(token.kind, Identifier | Literal))
        .map(|token| (token.text.as_str(), token.role))
        .collect();
    for expected in [
        ("load", Declaration),
        ("order", Parameter),
        ("scale", Parameter),
        ("Order", Type),
        ("i64", Type),
        ("Vec", Type),
        ("u8", Type),
        ("Point", Type),
        ("total", Member),
        ("saturating_mul", Member),
        ("x", Member),
        ("audit", Callee),
        ("println", Callee),
        ("item", Parameter),
        // Paths and locals are not classified.
        ("helpers", Unclassified),
        ("emit", Unclassified),
        ("f", Unclassified),
        ("\"{}\"", Lit(String)),
        ("'c'", Lit(Character)),
        ("true", Lit(Boolean)),
        ("1.5", Lit(Number)),
    ] {
        assert!(roles.contains(&expected), "{expected:?} in {roles:?}");
    }
    assert!(roles.contains(&("total", Unclassified)), "{roles:?}");
}

#[test]
fn callables_carry_outer_attributes_and_enclosing_constructs() {
    let source = "\
#[inline]
/// docs between attributes
#[must_use]
pub fn top() -> u8 { 1 }

#[cfg(test)]
mod tests {
    #![allow(dead_code)]
    #[test]
    fn case() {}
}

impl Point {
    fn method(&self) {
        let _ = || { fn in_closure() {} };
    }
}

trait Shape { fn area(&self) -> f64; }
";
    let file = tokenize(source);
    let spans = file.callables.unwrap();
    let summary: Vec<(&str, Vec<&str>, Vec<&str>)> = spans
        .iter()
        .map(|span| {
            (
                span.name.as_str(),
                span.decorators.iter().map(String::as_str).collect(),
                span.enclosing.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("top", vec!["#[inline]", "#[must_use]"], vec![]),
            ("case", vec!["#[test]"], vec!["#[cfg(test)] mod"]),
            ("method", vec![], vec!["impl"]),
            ("in_closure", vec![], vec!["impl", "fn", "closure"]),
            ("area", vec![], vec!["trait"]),
        ]
    );
}

fn body_shapes(file: &TokenizedFile) -> Vec<(String, Option<(usize, usize)>)> {
    file.callables
        .as_ref()
        .unwrap()
        .iter()
        .map(|span| {
            (
                span.name.clone(),
                span.body.map(|body| (body.statements, body.flat_calls)),
            )
        })
        .collect()
}

#[test]
fn body_shapes_count_flat_call_statements_without_reading_callee_names() {
    let source = "\
fn flat(x: u8) {
    // a comment
    log(x);
    self.items.push(x);
    println!(\"{} {}\", x, \"y\");
    #[allow(unused)]
    assert_eq!(x, 1);
    done(&x, Kind::A)
}

fn nested(x: u8) {
    log(compute(x));
    self.items().push(x);
    println!(\"{}\", f(x));
    run(|| x);
    log(if x { 1 } else { 2 });
    send(x)?;
    send(x).await;
}

fn other(x: u8) -> u8 {
    let y = log(x);
    return log(y);
}

trait T {
    fn signature(&self);
}
";
    assert_eq!(
        body_shapes(&tokenize(source)),
        [
            ("flat".to_string(), Some((5, 5))),
            ("nested".to_string(), Some((7, 0))),
            ("other".to_string(), Some((2, 0))),
            ("signature".to_string(), None),
        ]
    );
}

#[test]
fn primitive_spellings_require_complete_direct_annotations() {
    let file = tokenize(
        r#"
type u32 = String;
fn sample(a: u32, b: bool, c: Option<u8>, d: &str,
          e: std::primitive::u16, f: String) -> usize {
    let local: i64 = 1;
    0
}
trait T { fn sample(x: char) -> f64; }
"#,
    );
    assert!(file.error_lines.is_empty(), "{:?}", file.error_lines);
    let recognized: Vec<_> = file
        .tokens
        .iter()
        .filter(|token| token.primitive_spelling)
        .map(|token| token.text.as_str())
        .collect();
    assert_eq!(recognized, ["u32", "bool", "usize", "i64", "char", "f64"]);
    assert!(file
        .tokens
        .iter()
        .filter(|token| token.primitive_spelling)
        .all(|token| token.role == mct_core::TokenRole::Type));
}
