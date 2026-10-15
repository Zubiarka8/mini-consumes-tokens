//! Grammar-agnostic token extraction from a tree-sitter tree (issue #207).
//!
//! A language plugin describes which node kinds are trivia, literals and
//! identifiers ([`TokenClasses`]); [`tokenize_tree`] does the rest. Every
//! token comes from a node of the real syntax tree, never from a regular
//! expression over the source, so strings, comments and nesting are always
//! decided by the grammar.

use mct_core::{LineRange, ParseError, Token, TokenKind, TokenizedFile};
use tree_sitter::{Language, Node, Parser, Tree};

/// How one grammar's node kinds map to [`TokenKind`]s.
///
/// Anything not listed is classified structurally: a leaf made only of
/// letters and underscores is a keyword, any other leaf is punctuation.
#[derive(Debug, Clone, Copy)]
pub struct TokenClasses {
    /// Kinds whose whole subtree is dropped: comments (a Rust doc comment has
    /// child nodes), other nodes with no lexical content, such as a Python
    /// line continuation, and nodes that do not change what code does, such
    /// as a Rust visibility.
    pub trivia_kinds: &'static [&'static str],
    /// Kinds that become exactly one [`TokenKind::Literal`] covering the
    /// whole node, without looking at its children (a string with escape or
    /// interpolation children is still one token).
    pub literal_kinds: &'static [&'static str],
    /// Named leaf kinds that become [`TokenKind::Identifier`].
    pub identifier_kinds: &'static [&'static str],
    /// Leaf kinds that are keywords although their text is not only letters,
    /// such as Rust's single `macro_rules!` token.
    pub keyword_kinds: &'static [&'static str],
    /// Kinds of blocks that only layout delimits, such as Python's indented
    /// `block`, whose indent/dedent markers never appear in the tree. Each
    /// one with children is bracketed by a [`TokenKind::BlockStart`] and a
    /// [`TokenKind::BlockEnd`]. Empty for a grammar whose blocks have
    /// delimiter tokens of their own (braces, `end`).
    pub block_kinds: &'static [&'static str],
}

/// Parses `source` with `language` and extracts its tokens.
///
/// `Err` only when the grammar cannot be loaded or tree-sitter returns no
/// tree; syntax errors in `source` are reported in
/// [`TokenizedFile::error_lines`].
pub fn tokenize_source(
    language: &Language,
    classes: &TokenClasses,
    relative_path: &str,
    source: &str,
) -> Result<TokenizedFile, ParseError> {
    let failure = |message: &str| ParseError::Syntax {
        path: relative_path.to_string(),
        line: 1,
        message: message.to_string(),
    };
    let mut parser = Parser::new();
    parser
        .set_language(language)
        .map_err(|_| failure("tree-sitter grammar could not be loaded"))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| failure("tree-sitter produced no parse tree"))?;
    Ok(tokenize_tree(&tree, source, classes))
}

/// Extracts the tokens of `tree`, which must have been parsed from `source`.
///
/// The walk is iterative over a [`tree_sitter::TreeCursor`], so nesting depth
/// costs heap, never stack. It emits, in source order:
/// - one [`TokenKind::Literal`] per node of a literal kind;
/// - one token per remaining leaf: [`TokenKind::Identifier`] for a named leaf
///   of an identifier kind, [`TokenKind::Keyword`] for a keyword kind or when
///   its text is only letters and underscores, [`TokenKind::Punctuation`]
///   otherwise;
/// - a zero-width [`TokenKind::BlockStart`] at the start and a
///   [`TokenKind::BlockEnd`] at the end of every node of a block kind.
///
/// Trivia subtrees, zero-width leaves (`MISSING` nodes) and whitespace-only
/// leaves produce nothing. Tokens inside `ERROR` nodes are still emitted, but
/// those nodes' lines (and every `MISSING` node's line) land in
/// [`TokenizedFile::error_lines`], merged and sorted.
pub fn tokenize_tree(tree: &Tree, source: &str, classes: &TokenClasses) -> TokenizedFile {
    let mut out = TokenizedFile::default();
    let mut cursor = tree.walk();
    // Depth of the `ERROR` node being descended into, so nested errors do not
    // add redundant ranges.
    let mut depth = 0usize;
    let mut error_depth: Option<usize> = None;

    'walk: loop {
        let node = cursor.node();
        let descend = visit(node, source, classes, &mut out, &mut error_depth, depth);
        if descend && cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if depth == 0 || !cursor.goto_parent() {
                break 'walk;
            }
            depth -= 1;
            // Every child of the parent has been walked: the parent ends here.
            // Only a node with children is descended into, so this closes
            // exactly the blocks `visit` opened.
            let parent = cursor.node();
            if classes.block_kinds.contains(&parent.kind()) {
                push_block_marker(parent, TokenKind::BlockEnd, &mut out);
            }
            if error_depth == Some(depth) {
                error_depth = None;
            }
        }
    }

    out.error_lines = merge_ranges(std::mem::take(&mut out.error_lines));
    out
}

/// Handles one node; returns whether its children still have to be walked.
fn visit(
    node: Node,
    source: &str,
    classes: &TokenClasses,
    out: &mut TokenizedFile,
    error_depth: &mut Option<usize>,
    depth: usize,
) -> bool {
    if node.is_missing() {
        // A zero-width node at column 0 stands for a token expected before
        // the line break, so the line above is implicated too.
        let mut range = line_range(node);
        if node.start_position().column == 0 {
            range.start = range.start.saturating_sub(1).max(1);
        }
        out.error_lines.push(range);
        return false;
    }
    if node.is_error() && error_depth.is_none() {
        out.error_lines.push(line_range(node));
        if node.child_count() > 0 {
            *error_depth = Some(depth);
        }
    }

    let kind = node.kind();
    if classes.trivia_kinds.contains(&kind) {
        record_skipped_errors(node, out);
        return false;
    }
    if classes.literal_kinds.contains(&kind) {
        record_skipped_errors(node, out);
        push_token(node, TokenKind::Literal, source, out);
        return false;
    }
    if node.child_count() > 0 {
        if classes.block_kinds.contains(&kind) {
            push_block_marker(node, TokenKind::BlockStart, out);
        }
        return true;
    }

    // A zero-width leaf has no text; whitespace-only text carries nothing.
    let Some(text) = source.get(node.start_byte()..node.end_byte()) else {
        out.error_lines.push(line_range(node));
        return false;
    };
    if text.trim().is_empty() {
        return false;
    }
    let token_kind = if node.is_named() && classes.identifier_kinds.contains(&kind) {
        TokenKind::Identifier
    } else if classes.keyword_kinds.contains(&kind)
        || text.chars().all(|c| c.is_alphabetic() || c == '_')
    {
        TokenKind::Keyword
    } else {
        TokenKind::Punctuation
    };
    push_token(node, token_kind, source, out);
    false
}

/// A node whose subtree is not walked may still hide syntax errors.
fn record_skipped_errors(node: Node, out: &mut TokenizedFile) {
    if node.has_error() {
        out.error_lines.push(line_range(node));
    }
}

fn push_token(node: Node, kind: TokenKind, source: &str, out: &mut TokenizedFile) {
    let Some(text) = source.get(node.start_byte()..node.end_byte()) else {
        out.error_lines.push(line_range(node));
        return;
    };
    let range = line_range(node);
    out.tokens.push(Token {
        kind,
        text: text.to_string(),
        line: range.start,
        end_line: range.end,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    });
}

/// A zero-width marker at the start ([`TokenKind::BlockStart`]) or end
/// ([`TokenKind::BlockEnd`]) of `block`, on that edge's line, so markers stay
/// in `(line, start_byte)` order with the tokens around them.
fn push_block_marker(block: Node, kind: TokenKind, out: &mut TokenizedFile) {
    let range = line_range(block);
    let (line, byte) = if kind == TokenKind::BlockEnd {
        (range.end, block.end_byte())
    } else {
        (range.start, block.start_byte())
    };
    out.tokens.push(Token {
        kind,
        text: String::new(),
        line,
        end_line: line,
        start_byte: byte,
        end_byte: byte,
    });
}

/// `node`'s inclusive 1-based line range. A node ending right after a line
/// break ends on that line, not on the empty line after it.
fn line_range(node: Node) -> LineRange {
    let start = node.start_position();
    let end = node.end_position();
    let end_row = if end.column == 0 && end.row > start.row {
        end.row - 1
    } else {
        end.row
    };
    LineRange {
        start: to_line(start.row),
        end: to_line(end_row),
    }
}

fn to_line(row: usize) -> u32 {
    u32::try_from(row).map_or(u32::MAX, |row| row.saturating_add(1))
}

/// Sorts `ranges` and merges the overlapping ones.
fn merge_ranges(mut ranges: Vec<LineRange>) -> Vec<LineRange> {
    ranges.sort_unstable_by_key(|range| (range.start, range.end));
    let mut merged: Vec<LineRange> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    // Bash node kinds: `comment`, `string` (with `string_content` and
    // expansion children), `number`; commands and arguments are `word` leaves.
    const BASH: TokenClasses = TokenClasses {
        trivia_kinds: &["comment"],
        literal_kinds: &["string", "number"],
        identifier_kinds: &["variable_name", "word"],
        keyword_kinds: &[],
        block_kinds: &[],
    };

    fn bash(source: &str) -> TokenizedFile {
        tokenize_source(&tree_sitter_bash::LANGUAGE.into(), &BASH, "t.sh", source).unwrap()
    }

    /// Bash's `{ ...; }` group stands in for a layout-only block: the markers
    /// are emitted the same way whatever tokens the block has of its own.
    const BASH_WITH_BLOCKS: TokenClasses = TokenClasses {
        block_kinds: &["compound_statement"],
        ..BASH
    };

    #[test]
    fn block_kinds_are_bracketed_by_zero_width_markers_in_order() {
        let source = "{ echo a; { echo b; }; }\necho c\n";
        let file = tokenize_source(
            &tree_sitter_bash::LANGUAGE.into(),
            &BASH_WITH_BLOCKS,
            "t.sh",
            source,
        )
        .unwrap();
        let kinds: Vec<TokenKind> = file.tokens.iter().map(|token| token.kind).collect();
        let starts: Vec<usize> = kinds
            .iter()
            .enumerate()
            .filter(|(_, kind)| **kind == TokenKind::BlockStart)
            .map(|(i, _)| i)
            .collect();
        let ends: Vec<usize> = kinds
            .iter()
            .enumerate()
            .filter(|(_, kind)| **kind == TokenKind::BlockEnd)
            .map(|(i, _)| i)
            .collect();
        assert_eq!((starts.len(), ends.len()), (2, 2), "{kinds:?}");
        // Nested, not sequential: outer start, inner start, inner end, outer end.
        assert!(starts[0] < starts[1] && starts[1] < ends[0] && ends[0] < ends[1]);
        // The inner block holds `echo b` and the outer one closes before `echo c`.
        assert_eq!(file.tokens[ends[1] + 1].text, "echo");
        assert_eq!(file.tokens[ends[1] + 1].line, 2);
        for marker in file.tokens.iter().filter(|t| t.kind.is_block_marker()) {
            assert!(marker.text.is_empty());
            assert_eq!(marker.start_byte, marker.end_byte);
            assert_eq!((marker.line, marker.end_line), (1, 1));
        }
        assert!(file
            .tokens
            .windows(2)
            .all(|w| (w[0].line, w[0].end_byte) <= (w[1].line, w[1].start_byte)));
    }

    #[test]
    fn without_block_kinds_no_marker_is_emitted() {
        let file = bash("{ echo a; }\n");
        assert!(file.tokens.iter().all(|t| !t.kind.is_block_marker()));
    }

    fn kinds_and_texts(file: &TokenizedFile) -> Vec<(TokenKind, &str)> {
        file.tokens
            .iter()
            .map(|token| (token.kind, token.text.as_str()))
            .collect()
    }

    #[test]
    fn classifies_keywords_identifiers_literals_and_punctuation() {
        let file = bash("if [ \"$x\" = 1 ]; then echo hi; fi\n");
        assert!(file.error_lines.is_empty());
        let tokens = kinds_and_texts(&file);
        assert!(tokens.contains(&(TokenKind::Keyword, "if")));
        assert!(tokens.contains(&(TokenKind::Keyword, "then")));
        assert!(tokens.contains(&(TokenKind::Keyword, "fi")));
        assert!(tokens.contains(&(TokenKind::Literal, "\"$x\"")));
        assert!(tokens.contains(&(TokenKind::Literal, "1")));
        assert!(tokens.contains(&(TokenKind::Identifier, "echo")));
        assert!(tokens.contains(&(TokenKind::Punctuation, ";")));
        assert!(tokens.contains(&(TokenKind::Punctuation, "[")));
    }

    #[test]
    fn comments_and_whitespace_produce_no_tokens() {
        let file = bash("# only a comment\n\n   \necho a # trailing\n");
        assert_eq!(
            kinds_and_texts(&file),
            [
                (TokenKind::Identifier, "echo"),
                (TokenKind::Identifier, "a")
            ]
        );
    }

    #[test]
    fn a_multi_line_literal_is_one_token_with_both_lines() {
        let source = "echo 1\necho \"a\nb\nc\"\n";
        let file = bash(source);
        let string = file
            .tokens
            .iter()
            .find(|token| token.kind == TokenKind::Literal && token.text.starts_with('"'))
            .unwrap();
        assert_eq!((string.line, string.end_line), (2, 4));
        assert_eq!(&source[string.start_byte..string.end_byte], string.text);
    }

    #[test]
    fn tokens_come_in_source_order_with_one_based_lines() {
        let file = bash("echo a\necho b\n");
        let lines: Vec<u32> = file.tokens.iter().map(|token| token.line).collect();
        assert_eq!(lines, [1, 1, 2, 2]);
        assert!(file
            .tokens
            .windows(2)
            .all(|w| w[0].end_byte <= w[1].start_byte));
    }

    #[test]
    fn missing_and_error_nodes_are_reported_by_line() {
        // The subshell opened on line 3 is never closed: a MISSING `)`.
        let file = bash("echo a\necho b\nfoo (\necho c\n");
        assert!(
            file.error_lines
                .iter()
                .any(|range| range.start <= 4 && range.end >= 3),
            "{:?}",
            file.error_lines
        );
        assert!(!file.tokens.is_empty());
    }

    #[test]
    fn error_ranges_are_sorted_and_merged() {
        let merged = merge_ranges(vec![
            LineRange { start: 5, end: 6 },
            LineRange { start: 1, end: 2 },
            LineRange { start: 2, end: 3 },
            LineRange { start: 6, end: 6 },
        ]);
        assert_eq!(
            merged,
            [
                LineRange { start: 1, end: 3 },
                LineRange { start: 5, end: 6 }
            ]
        );
    }

    #[test]
    fn deeply_nested_source_does_not_overflow_the_stack() {
        let depth = 20_000;
        let source = format!("echo {}x{}\n", "$(echo ".repeat(depth), ")".repeat(depth));
        let handle = std::thread::Builder::new()
            // A deliberately small stack: a recursive walk would overflow it.
            .stack_size(256 * 1024)
            .spawn(move || bash(&source).tokens.len())
            .unwrap();
        assert!(handle.join().unwrap() > depth);
    }
}
