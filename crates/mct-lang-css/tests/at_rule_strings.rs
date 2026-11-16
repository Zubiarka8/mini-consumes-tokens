//! A statement at-rule on one line whose prelude is a string, a selector list
//! or a function (`@source "./";`, `@custom-variant dark (&:is(.d *));`,
//! `@source inline("x");`: Tailwind 4's directives) is valid CSS, but the
//! pinned grammar rejects it and the whole file was dropped. The parser
//! reparses with the prelude's bytes replaced, at the same offsets, and keeps
//! the tree only when it is clean.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, SourceFile, SymbolKind};
use mct_lang_css::CssParser;

fn parse(source: &str) -> Result<mct_core::ParsedFile, mct_core::ParseError> {
    CssParser.parse(&SourceFile {
        relative_path: "styles.css".to_string(),
        contents: source.to_string(),
    })
}

#[test]
fn a_statement_at_rule_with_a_string_prelude_parses() {
    let parsed = parse(
        "@import \"base.css\";\n@source \"./\";\n/* c */\n@source not \"./notes\";\n@plugin 'x';\n.btn { color: red; }\n",
    )
    .unwrap();
    assert!(
        parsed
            .symbols
            .iter()
            .any(|s| s.name == ".btn" && s.kind == SymbolKind::Rule),
        "{:?}",
        parsed.symbols
    );
    // The import is still read from the original text.
    assert!(
        parsed
            .relations
            .iter()
            .any(|r| r.to_name.contains("base.css")),
        "{:?}",
        parsed.relations
    );
}

#[test]
fn a_statement_at_rule_with_any_prelude_on_one_line_parses() {
    // Tailwind 4 directives whose prelude is a selector list or a function.
    let parsed = parse(
        "@custom-variant dark (&:where(.dark, .dark *));\n@custom-variant rtl (&:is([dir=rtl] *));\n@source inline(\"underline\");\n@foo bar baz;\n.btn { color: red; }\n",
    )
    .unwrap();
    assert!(
        parsed.symbols.iter().any(|s| s.name == ".btn"),
        "{:?}",
        parsed.symbols
    );
}

#[test]
fn a_real_syntax_error_is_still_rejected() {
    assert!(parse("@source \"./\";\n.btn { color: red;\n").is_err());
    assert!(parse("@source \"./\";\n.btn { ;;; } }\n").is_err());
    assert!(parse("@custom-variant dark (&:is(.d *));\n.btn { color: red;\n").is_err());
}
