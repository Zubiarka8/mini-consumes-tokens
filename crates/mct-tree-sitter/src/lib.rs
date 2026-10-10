//! Tree-sitter helpers shared by the `mct-lang-*` language plugins.
//!
//! This crate knows tree-sitter; `mct-core` does not, so the helpers live
//! here instead of there. Every helper was identical in each plugin before
//! it moved here (finding F08).

use mct_core::{Location, MAX_TRAVERSAL_DEPTH};
use std::ops::Range;
use tree_sitter::Node;

/// The first `ERROR` or `MISSING` node in `node`'s subtree, in document order.
///
/// Iterative, with the depth capped at `MAX_TRAVERSAL_DEPTH`: a deeper subtree
/// is not searched, so pathological input cannot overflow the stack.
pub fn first_error(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    let mut depth = 0u32;

    loop {
        let current = cursor.node();
        if current.is_error() || current.is_missing() {
            return Some(current);
        }

        if depth < MAX_TRAVERSAL_DEPTH && cursor.goto_first_child() {
            depth += 1;
            continue;
        }

        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if depth == 0 || !cursor.goto_parent() {
                return None;
            }
            depth -= 1;
        }
    }
}

/// `node`'s span as a `Location`: 1-based line and column of its start, its
/// byte length, and its 1-based end line.
pub fn location(node: Node) -> Location {
    let start = node.start_position();
    let end = node.end_position();
    Location {
        line: start.row as u32 + 1,
        column: start.column as u32 + 1,
        byte_len: (node.end_byte() - node.start_byte()) as u32,
        end_line: Some(end.row as u32 + 1),
    }
}

/// `contents` with every byte outside the `keep` byte ranges replaced by a
/// space, except `\n` and `\r`. The result has the same byte length and
/// line breaks, so a sub-parser's lines and byte columns over it are the
/// original file's (ADR-003 D4). A multi-byte char is kept whole when its
/// first byte is in `keep`, else replaced by one space per byte, so the
/// result is always valid UTF-8.
pub fn mask(contents: &str, keep: &[Range<usize>]) -> String {
    let mut out = String::with_capacity(contents.len());
    for (i, c) in contents.char_indices() {
        if c == '\n' || c == '\r' || keep.iter().any(|r| r.contains(&i)) {
            out.push(c);
        } else {
            out.extend(std::iter::repeat_n(' ', c.len_utf8()));
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use tree_sitter::{Parser, Tree};

    fn parse_bash(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_bash::LANGUAGE.into())
            .expect("bash grammar loads");
        parser.parse(source, None).expect("source parses")
    }

    #[test]
    fn first_error_is_none_for_valid_source() {
        let tree = parse_bash("echo ok\n");
        assert!(first_error(tree.root_node()).is_none());
    }

    #[test]
    fn first_error_finds_a_missing_token() {
        // `foo (` opens a subshell that is never closed: tree-sitter inserts a
        // MISSING `)` node rather than an ERROR node.
        let source = "echo a\necho b\nfoo (\necho c\n";
        let tree = parse_bash(source);
        let error = first_error(tree.root_node()).expect("source has an error");
        assert!(error.is_missing());
        assert_eq!(error.kind(), ")");
    }

    #[test]
    fn location_is_one_based_and_spans_the_node() {
        let source = "echo a\n  echo bb\n";
        let tree = parse_bash(source);
        // `bb` is bytes 14..16: line 2, column 8 (1-based), two bytes long.
        let node = tree
            .root_node()
            .descendant_for_byte_range(14, 16)
            .expect("node covers bytes 14..16");
        assert_eq!(node.utf8_text(source.as_bytes()), Ok("bb"));
        assert_eq!(
            location(node),
            Location {
                line: 2,
                column: 8,
                byte_len: 2,
                end_line: Some(2),
            }
        );
    }

    #[test]
    // One keep range per call is intended: `&[a..b]` is a slice of ranges.
    #[allow(clippy::single_range_in_vec_init)]
    fn mask_blanks_outside_keep_and_preserves_layout() {
        let source = "---\nlet a = 1;\n---\r\n<p>é</p>\n";
        let masked = mask(source, &[4..15]);
        assert_eq!(masked.len(), source.len());
        assert_eq!(masked, "   \nlet a = 1;\n   \r\n         \n");
        assert_eq!(mask(source, &[]).lines().count(), source.lines().count());
        // A range starting mid-char keeps no partial char.
        assert_eq!(mask("é!", &[1..3]), "  !");
        assert_eq!(mask("aé", &[1..2]), "aé".replace('a', " "));
    }
}
