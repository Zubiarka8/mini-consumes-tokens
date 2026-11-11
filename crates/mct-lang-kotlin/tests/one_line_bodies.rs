//! A body closed on the line its last member ends on is valid Kotlin, but the
//! pinned grammar wants a terminator there (`MISSING _class_member_semi`, or
//! an `ERROR` for a one-line enum with a constant body). Such a file is
//! indexed like its multi-line spelling, at the original offsets. A file
//! with a real syntax error is still rejected.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, ParseError, ParsedFile, SourceFile};
use mct_lang_kotlin::KotlinParser;

fn parse(contents: &str) -> Result<ParsedFile, ParseError> {
    KotlinParser.parse(&SourceFile {
        relative_path: "Shapes.kt".into(),
        contents: contents.into(),
    })
}

/// Every symbol as `kind name line:column+len`, and every relation as
/// `kind to line:column`.
fn shape(parsed: &ParsedFile) -> Vec<String> {
    let symbols = parsed.symbols.iter().skip(1).map(|s| {
        format!(
            "{:?} {} {}:{}+{}",
            s.kind, s.name, s.location.line, s.location.column, s.location.byte_len
        )
    });
    let relations = parsed.relations.iter().map(|r| {
        format!(
            "{:?} {} {}:{}",
            r.kind, r.to_name, r.location.line, r.location.column
        )
    });
    symbols.chain(relations).collect()
}

const ONE_LINE: &str = r#"interface Shape { fun area(): Double }
class Square(val side: Double) : Shape { override fun area() = side * side }
enum class Op { ADD { override fun apply() = 1 }; abstract fun apply(): Int }
class Holder { val size: Int }
fun start() { val task = object : Runnable { override fun run() { log("${ "x" }") } } }
"#;

#[test]
fn a_body_closed_on_its_last_line_is_indexed() {
    assert_eq!(
        shape(&parse(ONE_LINE).unwrap()),
        [
            "Interface Shape 1:1+38",
            "Method area 1:19+18",
            "Class Square 2:1+76",
            "Field side 2:14+16",
            "Method area 2:42+33",
            "Class Op 3:1+77",
            "Field ADD 3:17+32",
            "Method apply 3:23+24",
            "Method apply 3:51+25",
            "Class Holder 4:1+30",
            "Field size 4:16+13",
            "Function start 5:1+87",
            "Function run 5:46+38",
            "Implements Shape 2:34",
            "Overrides area 2:55",
            "Overrides apply 3:36",
            "Overrides run 5:59",
            "Calls log 5:67",
        ]
    );
}

#[test]
fn locations_are_those_of_the_original_source() {
    let source = "class A { fun f() = 1 }";
    let parsed = parse(source).unwrap();
    let class = parsed.symbols.iter().find(|s| s.name == "A").unwrap();
    assert_eq!(class.location.byte_len as usize, source.len());
    let f = parsed.symbols.iter().find(|s| s.name == "f").unwrap();
    assert_eq!((f.location.column, f.location.byte_len), (11, 11));
}

#[test]
fn a_one_line_body_has_the_shape_of_its_multi_line_spelling() {
    let one_line = "interface Shape { fun area(): Double }\nclass Square : Shape { override fun area() = 1.0 }\n";
    let spread = "interface Shape { fun area(): Double\n}\nclass Square : Shape { override fun area() = 1.0\n}\n";
    let lines = |p: &ParsedFile| {
        let symbols = p
            .symbols
            .iter()
            .skip(1)
            .map(|s| format!("{:?} {} {:?}", s.kind, s.name, s.parent));
        let relations = p
            .relations
            .iter()
            .map(|r| format!("{:?} {}", r.kind, r.to_name));
        symbols.chain(relations).collect::<Vec<_>>()
    };
    assert_eq!(
        lines(&parse(one_line).unwrap()),
        lines(&parse(spread).unwrap())
    );
}

#[test]
fn a_real_syntax_error_is_still_rejected() {
    for source in [
        "class A { fun f( }",
        "class A { fun f() = }",
        "fun g() { val s = \"${ a }\" + }",
    ] {
        assert!(
            matches!(parse(source), Err(ParseError::Syntax { .. })),
            "{source}"
        );
    }
}

#[test]
fn the_documented_one_line_forms_parse() {
    // docs/02-crates/parsers/symbol-coverage.md, "Valid code the grammar
    // rejects".
    for (source, member) in [
        ("class A { fun f() {} }", "f"),
        ("class A { val x = 1 }", "x"),
        ("object O { fun f() {} }", "f"),
    ] {
        let parsed = parse(source).unwrap();
        assert!(parsed.symbols.iter().any(|s| s.name == member), "{source}");
    }
}
