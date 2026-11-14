//! String literals for exact-text search: each run of fixed text of a
//! string, verbatim (`@"…"`), raw (`"""…"""`) or interpolated (`$"…"`)
//! string between `{…}` holes, through the shared prose filter. A verbatim
//! string keeps its backslashes and reads `""` as one quote; a raw string
//! keeps its backslashes; `{{` in a non-raw interpolated string is one
//! brace.

use mct_core::StringLiteral;
use mct_tree_sitter::{LiteralGrammar, StringStyle};
use tree_sitter::Node;

const GRAMMAR: LiteralGrammar = LiteralGrammar {
    strings: &[
        "string_literal",
        "verbatim_string_literal",
        "raw_string_literal",
        "interpolated_string_expression",
    ],
    fragments: &[
        "string_literal_content",
        "raw_string_content",
        "string_content",
    ],
    escapes: &["escape_sequence"],
    holes: &["interpolation"],
    concatenations: &[],
    statements: &[],
    opaque: &["verbatim_string_literal"],
    dollar_names: false,
};

/// Every literal of the file under `root`, in source order.
pub(crate) fn collect(root: Node, source: &str) -> Vec<StringLiteral> {
    mct_tree_sitter::collect_literals(root, source, &GRAMMAR, |string| Some(style(string, source)))
}

fn style(string: Node, source: &str) -> StringStyle {
    match string.kind() {
        "verbatim_string_literal" => StringStyle {
            raw: true,
            doubled_braces: false,
            doubled_quotes: true,
        },
        "raw_string_literal" => StringStyle {
            raw: true,
            ..StringStyle::default()
        },
        "interpolated_string_expression" => {
            let mut cursor = string.walk();
            let mut verbatim = false;
            let mut raw = false;
            for child in string.children(&mut cursor) {
                match child.kind() {
                    "interpolation_start" => {
                        verbatim = child
                            .utf8_text(source.as_bytes())
                            .is_ok_and(|t| t.contains('@'));
                    }
                    "interpolation_quote" => raw = true,
                    _ => {}
                }
            }
            StringStyle {
                raw: verbatim || raw,
                doubled_braces: !raw,
                doubled_quotes: verbatim,
            }
        }
        _ => StringStyle::default(),
    }
}
