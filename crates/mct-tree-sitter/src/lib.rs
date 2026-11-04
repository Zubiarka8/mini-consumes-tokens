//! Tree-sitter helpers shared by the `mct-lang-*` language plugins.
//!
//! This crate knows tree-sitter; `mct-core` does not, so the helpers live
//! here instead of there. Every helper was identical in each plugin before
//! it moved here (finding F08).

use mct_core::{
    Location, RelationKind, SymbolId, SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use std::ops::Range;
use tree_sitter::Node;

mod tokens;

pub use tokens::{tokenize_source, tokenize_tree, TokenClasses};

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

/// The span of a whole-file module node. A grammar's root node can end at
/// column 0 of the line after the last token; the module then ends on the
/// previous line, so `end_line` is the root's end row (already 1-based
/// there) instead of its end row plus one.
pub fn module_location(root: Node) -> Location {
    let mut loc = location(root);
    let end = root.end_position();
    if end.column == 0 && end.row > root.start_position().row {
        loc.end_line = Some(end.row as u32);
    }
    loc
}

/// The first child of `node`, named or anonymous, whose kind is `kind`.
pub fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children.into_iter().find(|n| n.kind() == kind)
}

/// The first named child of `node` whose kind is `kind`; anonymous tokens
/// such as punctuation are skipped.
pub fn find_named_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

/// Appends a symbol to `symbols` with the id `*next_id`, advances `*next_id`
/// by one and returns the new id. The counter is explicit, not derived from
/// `symbols.len()`, so a walker keeps control of its own id sequence.
pub fn push_symbol(
    symbols: &mut Vec<SymbolRecord>,
    next_id: &mut SymbolId,
    name: String,
    kind: SymbolKind,
    location: Location,
    parent: Option<String>,
) -> SymbolId {
    let id = *next_id;
    *next_id += 1;
    symbols.push(SymbolRecord {
        id,
        name,
        kind,
        location,
        parent,
        level: None,
    });
    id
}

/// Appends a relation from `from` to `to_name` to `relations`; an empty
/// `to_name` has nothing to resolve and is dropped.
pub fn push_relation(
    relations: &mut Vec<SymbolRelation>,
    from: SymbolId,
    kind: RelationKind,
    to_name: String,
    location: Location,
) {
    if to_name.is_empty() {
        return;
    }
    relations.push(SymbolRelation {
        from,
        kind,
        to_name,
        location,
    });
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

    #[test]
    fn module_location_ends_on_the_last_line_with_content() {
        let tree = parse_bash("echo ok\n");
        let root = tree.root_node();
        assert_eq!(module_location(root).end_line, Some(1));
        assert_eq!(location(root).end_line, Some(2));
    }

    #[test]
    fn find_child_sees_anonymous_tokens_and_find_named_child_does_not() {
        let tree = parse_bash("(echo ok)\n");
        let subshell = tree
            .root_node()
            .named_child(0)
            .expect("the file has a subshell");
        assert_eq!(subshell.kind(), "subshell");
        assert!(find_child(subshell, "(").is_some());
        assert!(find_named_child(subshell, "(").is_none());
        assert!(find_named_child(subshell, "command").is_some());
    }

    #[test]
    fn push_symbol_assigns_sequential_ids_from_the_explicit_counter() {
        let loc = Location {
            line: 1,
            column: 1,
            byte_len: 0,
            end_line: Some(1),
        };
        let mut symbols = Vec::new();
        // A counter that does not match `symbols.len()` is honoured.
        let mut next_id: SymbolId = 5;
        let first = push_symbol(
            &mut symbols,
            &mut next_id,
            "a".into(),
            SymbolKind::Module,
            loc,
            None,
        );
        let second = push_symbol(
            &mut symbols,
            &mut next_id,
            "b".into(),
            SymbolKind::Function,
            loc,
            Some("a".into()),
        );
        assert_eq!((first, second, next_id), (5, 6, 7));
        assert_eq!(symbols[1].parent.as_deref(), Some("a"));
        assert_eq!(symbols[1].level, None);
    }

    #[test]
    fn push_relation_drops_an_empty_target_name() {
        let loc = Location {
            line: 1,
            column: 1,
            byte_len: 0,
            end_line: Some(1),
        };
        let mut relations = Vec::new();
        push_relation(&mut relations, 0, RelationKind::Calls, String::new(), loc);
        assert!(relations.is_empty());
        push_relation(&mut relations, 0, RelationKind::Calls, "f".into(), loc);
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].to_name, "f");
    }
}
