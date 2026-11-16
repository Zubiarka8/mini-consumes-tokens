//! String literals for exact-text search: each run of fixed text of a
//! double-quoted string between `$x`, `${…}`, `$(…)` and `$((…))`
//! expansions, and each single-quoted or ANSI-C (`$'…'`) string, through
//! the shared prose filter. In `"…"` only `\"`, `\\`, `` \` ``, `\$` and a
//! backslash-newline are escapes; `'…'` has none; `$'…'` resolves C
//! escapes. A heredoc body is not read: the grammar leaves its text before
//! the first expansion outside any node, and a quoted heredoc has no
//! children.

use mct_core::StringLiteral;
use mct_tree_sitter::{LiteralGrammar, StringStyle};
use tree_sitter::Node;

const GRAMMAR: LiteralGrammar = LiteralGrammar {
    strings: &["string", "raw_string", "ansi_c_string"],
    fragments: &["string_content"],
    escapes: &[],
    holes: &[
        "simple_expansion",
        "expansion",
        "command_substitution",
        "arithmetic_expansion",
    ],
    concatenations: &[],
    statements: &[],
    opaque: &["raw_string", "ansi_c_string"],
    dollar_names: false,
};

/// Every literal of the file under `root`, in source order.
pub(crate) fn collect(root: Node, source: &str) -> Vec<StringLiteral> {
    mct_tree_sitter::collect_literals(root, source, &GRAMMAR, |string| {
        Some(match string.kind() {
            "raw_string" => StringStyle {
                raw: true,
                ..StringStyle::default()
            },
            "ansi_c_string" => StringStyle::default(),
            _ => StringStyle {
                shell_escapes: true,
                ..StringStyle::default()
            },
        })
    })
}
