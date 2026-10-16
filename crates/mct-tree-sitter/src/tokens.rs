//! Grammar-agnostic token extraction from a tree-sitter tree (issue #207).
//!
//! A language plugin describes which node kinds are trivia, literals and
//! identifiers ([`TokenClasses`]); [`tokenize_tree`] does the rest. Every
//! token comes from a node of the real syntax tree, never from a regular
//! expression over the source, so strings, comments and nesting are always
//! decided by the grammar.

use mct_core::{
    BodyShape, CallableSpan, LineRange, LiteralCategory, ParseError, Token, TokenKind, TokenRole,
    TokenizedFile,
};
use tree_sitter::{Language, Node, Parser, Tree};

/// How one grammar's node kinds map to [`TokenKind`]s.
///
/// Anything not listed is classified structurally: a leaf made only of
/// letters and underscores is a keyword, any other leaf is punctuation.
#[derive(Debug, Clone, Copy)]
pub struct TokenClasses {
    /// Callable nodes with a `name` field. Their exact ranges are returned.
    pub callable_kinds: &'static [&'static str],
    /// `(container, expression-child)` pairs. Expand a literal only when
    /// a direct child has the expression kind; ordinary strings stay atomic.
    pub expanded_literal_kinds: &'static [(&'static str, &'static str)],
    /// Kinds whose whole subtree is dropped: comments (a Rust doc comment has
    /// child nodes), other nodes with no lexical content, such as a Python
    /// line continuation, and nodes that do not change what code does, such
    /// as a Rust visibility.
    pub trivia_kinds: &'static [&'static str],
    /// Kinds that become exactly one [`TokenKind::Literal`] covering the
    /// whole node, unless `expanded_literal_kinds` opts its expression children
    /// into structural traversal.
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
    /// `(literal kind, category)`. A literal kind without an entry is
    /// [`LiteralCategory::Other`].
    pub literal_categories: &'static [(&'static str, LiteralCategory)],
    /// `(literal kind, child kind)`: such a literal with a direct child of that
    /// kind is a [`LiteralCategory::InterpolatedString`], in every mode.
    pub interpolated_literal_kinds: &'static [(&'static str, &'static str)],
    /// Parent kinds that make a literal child a whole statement whose value is
    /// unused ([`LiteralCategory::Docstring`]), such as Python's
    /// `expression_statement`.
    pub statement_literal_parents: &'static [&'static str],
    /// `(identifier kind, role)`: kinds whose role the kind alone decides,
    /// such as Rust's `type_identifier`. Checked first.
    pub kind_roles: &'static [(&'static str, TokenRole)],
    /// `(leaf kind, text)` recognized as primitive spellings by this language.
    /// Recognition never resolves names or establishes type identity.
    pub primitive_spellings: &'static [(&'static str, &'static str)],
    /// `(parent kind, field)` sites where a complete annotation is supported.
    pub annotation_sites: &'static [(&'static str, &'static str)],
    /// Transparent single-child annotation wrappers, such as Python's `type`.
    pub annotation_wrappers: &'static [&'static str],
    /// Kinds whose whole subtree is a type position, such as Python's `type`:
    /// an identifier inside one is a [`TokenRole::Type`]. Checked second.
    pub type_kinds: &'static [&'static str],
    /// `(parent kind, field name, role)` for an identifier leaf by its direct
    /// parent; an empty field name matches any child. Checked last.
    pub parent_roles: &'static [(&'static str, &'static str, TokenRole)],
    /// Kinds that `parent_roles` looks through, such as Python's
    /// `list_splat_pattern` around `*args`: an identifier whose direct parent
    /// is one is matched against its grandparent instead, and only by
    /// entries with an empty field name. The same kind under any other
    /// grandparent (`a, *rest = x`) stays unclassified.
    pub role_wrappers: &'static [&'static str],
    /// Kinds of decorators or outer attributes (Python's `decorator`, Rust's
    /// `attribute_item`). Such nodes, with only trivia between them, attach to
    /// the next sibling node: a callable's become [`CallableSpan::decorators`],
    /// an enclosing construct's prefix its context label.
    pub decorator_kinds: &'static [&'static str],
    /// `(kind, label)`: constructs reported in [`CallableSpan::enclosing`]
    /// when they enclose a callable, such as `("class_definition", "class")`.
    pub context_kinds: &'static [(&'static str, &'static str)],
    /// Kinds that wrap one expression as a statement, such as
    /// `expression_statement`. Body shapes look through them.
    pub expression_statement_kinds: &'static [&'static str],
    /// Call and macro-invocation kinds. Empty: no [`CallableSpan::body`] is
    /// reported.
    pub call_kinds: &'static [&'static str],
    /// Kinds that make a call's arguments non-flat besides a nested call:
    /// closures, comprehensions, conditionals, blocks, `await`, `?`.
    pub non_flat_kinds: &'static [&'static str],
    /// Delimited, unparsed argument kinds, such as Rust's macro `token_tree`.
    /// One nested in another makes the call non-flat, since a call inside
    /// it cannot be told apart from any other parenthesized tokens.
    pub delimited_argument_kinds: &'static [&'static str],
}

/// A node being walked: its kind, its context label when it is a context kind,
/// and decorators read among its children not yet attached to a sibling.
struct Frame {
    kind: &'static str,
    context: Option<String>,
    pending: Vec<String>,
}

/// Where the node being visited sits: its parent's and grandparent's kinds,
/// its field in that parent, whether an ancestor is a type position, the
/// decorators attached to it, and the nodes enclosing it.
#[derive(Clone, Copy)]
struct Place<'a> {
    parent: Option<&'static str>,
    grandparent: Option<&'static str>,
    field: Option<&'static str>,
    in_type: bool,
    decorators: &'a [String],
    enclosing: &'a [Frame],
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
    if !classes.callable_kinds.is_empty() {
        out.callables = Some(Vec::new());
    }
    let mut cursor = tree.walk();
    // Depth of the `ERROR` node being descended into, so nested errors do not
    // add redundant ranges.
    let mut depth = 0usize;
    let mut error_depth: Option<usize> = None;
    // The nodes descended into, innermost last, and how many of them are type
    // positions: roles and contexts need the ancestors without `Node::parent`'s
    // walk from the root.
    let mut ancestors: Vec<Frame> = Vec::new();
    let mut type_scopes = 0usize;

    'walk: loop {
        let node = cursor.node();
        let decorators = attach_decorators(node, source, classes, ancestors.last_mut());
        let place = Place {
            parent: ancestors.last().map(|frame| frame.kind),
            grandparent: ancestors.iter().rev().nth(1).map(|frame| frame.kind),
            field: cursor.field_name(),
            in_type: type_scopes > 0,
            decorators: &decorators,
            enclosing: &ancestors,
        };
        let descend = visit(
            node,
            place,
            source,
            classes,
            &mut out,
            &mut error_depth,
            depth,
        );
        if descend && cursor.goto_first_child() {
            depth += 1;
            let kind = node.kind();
            ancestors.push(Frame {
                kind,
                context: context_label(kind, &decorators, classes),
                pending: Vec::new(),
            });
            if classes.type_kinds.contains(&kind) {
                type_scopes += 1;
            }
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
            if ancestors
                .pop()
                .is_some_and(|frame| classes.type_kinds.contains(&frame.kind))
            {
                type_scopes = type_scopes.saturating_sub(1);
            }
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
    place: Place,
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
    if classes.callable_kinds.contains(&kind) {
        if let (Some(spans), Some(name)) =
            (out.callables.as_mut(), node.child_by_field_name("name"))
        {
            if let Some(text) = source.get(name.byte_range()) {
                spans.push(CallableSpan {
                    name: text.to_string(),
                    lines: line_range(node),
                    bytes: node.byte_range(),
                    decorators: place.decorators.to_vec(),
                    enclosing: place
                        .enclosing
                        .iter()
                        .filter_map(|frame| frame.context.clone())
                        .collect(),
                    body: body_shape(node, classes),
                });
            }
        }
    }
    if classes.trivia_kinds.contains(&kind) {
        record_skipped_errors(node, out);
        return false;
    }
    let expands_literal = classes
        .expanded_literal_kinds
        .iter()
        .any(|(container, expression)| kind == *container && has_child_kind(node, expression));
    if classes.literal_kinds.contains(&kind) && !expands_literal {
        record_skipped_errors(node, out);
        let role = TokenRole::Literal(literal_category(node, place, classes));
        push_token(node, TokenKind::Literal, role, source, out);
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
    if node.is_named() && classes.identifier_kinds.contains(&kind) {
        let role = identifier_role(kind, place, classes);
        push_token(node, TokenKind::Identifier, role, source, out);
        if role == TokenRole::Type && primitive_spelling(node, text, classes) {
            if let Some(token) = out.tokens.last_mut() {
                token.primitive_spelling = true;
            }
        }
        return false;
    }
    let token_kind = if classes.keyword_kinds.contains(&kind)
        || text.chars().all(|c| c.is_alphabetic() || c == '_')
    {
        TokenKind::Keyword
    } else {
        TokenKind::Punctuation
    };
    push_token(node, token_kind, TokenRole::Unclassified, source, out);
    false
}

/// Decorators attach to the next sibling that is neither a decorator nor
/// trivia. A decorator node is queued on its parent's frame and gets none;
/// trivia keeps the queue; any other node takes it.
fn attach_decorators(
    node: Node,
    source: &str,
    classes: &TokenClasses,
    parent: Option<&mut Frame>,
) -> Vec<String> {
    let Some(parent) = parent else {
        return Vec::new();
    };
    let kind = node.kind();
    if classes.decorator_kinds.contains(&kind) {
        parent
            .pending
            .push(text_without_trivia(node, source, classes));
        return Vec::new();
    }
    if classes.trivia_kinds.contains(&kind) {
        return Vec::new();
    }
    std::mem::take(&mut parent.pending)
}

/// `node`'s source text without its descendant trivia subtrees, whitespace
/// collapsed to single spaces. Iterative, like the main walk.
fn text_without_trivia(node: Node, source: &str, classes: &TokenClasses) -> String {
    let mut text = String::new();
    let mut copied = node.start_byte();
    let mut cursor = node.walk();
    let mut depth = 0usize;
    'walk: loop {
        let current = cursor.node();
        if depth > 0 && classes.trivia_kinds.contains(&current.kind()) {
            text.push_str(source.get(copied..current.start_byte()).unwrap_or(""));
            text.push(' ');
            copied = current.end_byte();
        } else if cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        loop {
            if depth > 0 && cursor.goto_next_sibling() {
                break;
            }
            if depth == 0 || !cursor.goto_parent() {
                break 'walk;
            }
            depth -= 1;
        }
    }
    text.push_str(source.get(copied..node.end_byte()).unwrap_or(""));
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `kind`'s context label, prefixed by its decorators, if it is a context kind.
fn context_label(kind: &str, decorators: &[String], classes: &TokenClasses) -> Option<String> {
    let (_, label) = classes.context_kinds.iter().find(|(k, _)| *k == kind)?;
    Some(if decorators.is_empty() {
        (*label).to_string()
    } else {
        format!("{} {label}", decorators.join(" "))
    })
}

/// The statement shape of `callable`'s `body` field. Direct statements are
/// its named children other than trivia and decorators; an expression
/// statement is looked through, and a literal one (a docstring) is not
/// counted.
fn body_shape(callable: Node, classes: &TokenClasses) -> Option<BodyShape> {
    if classes.call_kinds.is_empty() {
        return None;
    }
    let body = callable.child_by_field_name("body")?;
    let mut shape = BodyShape::default();
    let mut cursor = body.walk();
    for statement in body.named_children(&mut cursor) {
        let kind = statement.kind();
        if classes.trivia_kinds.contains(&kind) || classes.decorator_kinds.contains(&kind) {
            continue;
        }
        let expression = if classes.expression_statement_kinds.contains(&kind) {
            let mut inner = statement.walk();
            let mut children = statement
                .named_children(&mut inner)
                .filter(|child| !classes.trivia_kinds.contains(&child.kind()));
            match (children.next(), children.next()) {
                (Some(only), None) => Some(only),
                _ => None,
            }
        } else {
            Some(statement)
        };
        if expression.is_some_and(|expression| {
            classes.statement_literal_parents.contains(&kind)
                && classes.literal_kinds.contains(&expression.kind())
        }) {
            continue;
        }
        shape.statements += 1;
        if expression.is_some_and(|expression| is_flat_call(expression, classes)) {
            shape.flat_calls += 1;
        }
    }
    Some(shape)
}

/// Whether `node` is a call whose descendants hold no other call, no
/// non-flat kind and no delimited argument nested in another. Iterative.
fn is_flat_call(node: Node, classes: &TokenClasses) -> bool {
    if !classes.call_kinds.contains(&node.kind()) {
        return false;
    }
    let mut cursor = node.walk();
    let mut depth = 0usize;
    'walk: loop {
        let current = cursor.node();
        let kind = current.kind();
        if depth > 0
            && (classes.call_kinds.contains(&kind)
                || classes.non_flat_kinds.contains(&kind)
                || (classes.delimited_argument_kinds.contains(&kind)
                    && classes
                        .delimited_argument_kinds
                        .iter()
                        .any(|inner| has_child_kind(current, inner))))
        {
            return false;
        }
        if cursor.goto_first_child() {
            depth += 1;
            continue;
        }
        loop {
            if depth > 0 && cursor.goto_next_sibling() {
                break;
            }
            if depth == 0 || !cursor.goto_parent() {
                break 'walk;
            }
            depth -= 1;
        }
    }
    true
}

fn has_child_kind(node: Node, kind: &str) -> bool {
    (0..node.child_count()).any(|i| node.child(i).is_some_and(|child| child.kind() == kind))
}

/// A literal's category: statement position first, then interpolation, then
/// the kind table.
fn literal_category(node: Node, place: Place, classes: &TokenClasses) -> LiteralCategory {
    if place
        .parent
        .is_some_and(|parent| classes.statement_literal_parents.contains(&parent))
    {
        return LiteralCategory::Docstring;
    }
    let kind = node.kind();
    if classes
        .interpolated_literal_kinds
        .iter()
        .any(|(container, child)| kind == *container && has_child_kind(node, child))
    {
        return LiteralCategory::InterpolatedString;
    }
    classes
        .literal_categories
        .iter()
        .find(|(literal, _)| *literal == kind)
        .map_or(LiteralCategory::Other, |(_, category)| *category)
}

/// Recognized primitive spelling used as the whole annotation at a supported
/// site. Looking through wrappers does not inspect generics or resolve names.
fn primitive_spelling(node: Node, text: &str, classes: &TokenClasses) -> bool {
    if !classes.primitive_spellings.contains(&(node.kind(), text)) {
        return false;
    }
    let mut annotation = node;
    while let Some(parent) = annotation.parent() {
        if classes.annotation_wrappers.contains(&parent.kind()) && parent.named_child_count() == 1 {
            annotation = parent;
        } else {
            break;
        }
    }
    let Some(parent) = annotation.parent() else {
        return false;
    };
    classes.annotation_sites.iter().any(|(kind, field)| {
        parent.kind() == *kind && parent.child_by_field_name(field) == Some(annotation)
    })
}

/// An identifier's syntactic role; no binding or type identity is resolved.
fn identifier_role(kind: &str, place: Place, classes: &TokenClasses) -> TokenRole {
    if let Some((_, role)) = classes.kind_roles.iter().find(|(k, _)| *k == kind) {
        return *role;
    }
    if place.in_type {
        return TokenRole::Type;
    }
    // Through a wrapper the wrapper's own field is unknown, so only entries
    // that match any field can apply.
    let (parent, field) = match place.parent {
        Some(kind) if classes.role_wrappers.contains(&kind) => (place.grandparent, None),
        parent => (parent, place.field),
    };
    classes
        .parent_roles
        .iter()
        .find(|(kind, wanted, _)| {
            parent == Some(*kind) && (wanted.is_empty() || field == Some(*wanted))
        })
        .map_or(TokenRole::Unclassified, |(_, _, role)| *role)
}

/// A node whose subtree is not walked may still hide syntax errors.
fn record_skipped_errors(node: Node, out: &mut TokenizedFile) {
    if node.has_error() {
        out.error_lines.push(line_range(node));
    }
}

fn push_token(node: Node, kind: TokenKind, role: TokenRole, source: &str, out: &mut TokenizedFile) {
    let Some(text) = source.get(node.start_byte()..node.end_byte()) else {
        out.error_lines.push(line_range(node));
        return;
    };
    let range = line_range(node);
    out.tokens.push(Token {
        kind,
        role,
        primitive_spelling: false,
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
        role: TokenRole::Unclassified,
        primitive_spelling: false,
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
        callable_kinds: &[],
        expanded_literal_kinds: &[],
        trivia_kinds: &["comment"],
        literal_kinds: &["string", "number"],
        identifier_kinds: &["variable_name", "word"],
        keyword_kinds: &[],
        block_kinds: &[],
        literal_categories: &[("number", LiteralCategory::Number)],
        interpolated_literal_kinds: &[("string", "expansion")],
        statement_literal_parents: &[],
        kind_roles: &[("variable_name", TokenRole::Parameter)],
        primitive_spellings: &[],
        annotation_sites: &[],
        annotation_wrappers: &[],
        type_kinds: &[],
        parent_roles: &[("command_name", "", TokenRole::Callee)],
        role_wrappers: &[],
        decorator_kinds: &[],
        context_kinds: &[],
        expression_statement_kinds: &[],
        call_kinds: &[],
        non_flat_kinds: &[],
        delimited_argument_kinds: &[],
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
    fn primitive_spelling_tables_require_a_type_role_and_the_annotation_field() {
        let classes = TokenClasses {
            kind_roles: &[("variable_name", TokenRole::Type)],
            primitive_spellings: &[("variable_name", "x")],
            annotation_sites: &[("variable_assignment", "name")],
            ..BASH
        };
        let source = "x=1\necho $x\n";
        let file =
            tokenize_source(&tree_sitter_bash::LANGUAGE.into(), &classes, "t.sh", source).unwrap();
        assert_eq!(
            file.tokens
                .iter()
                .filter(|token| token.primitive_spelling)
                .count(),
            1
        );
        for classes in [
            TokenClasses {
                annotation_sites: &[("variable_assignment", "value")],
                ..classes
            },
            TokenClasses {
                kind_roles: &[],
                ..classes
            },
        ] {
            let file =
                tokenize_source(&tree_sitter_bash::LANGUAGE.into(), &classes, "t.sh", source)
                    .unwrap();
            assert!(file.tokens.iter().all(|token| !token.primitive_spelling));
        }
    }

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

    /// The roles in `BASH` are arbitrary stand-ins; only the precedence and
    /// table mechanics are under test here.
    #[test]
    fn roles_come_from_kind_parent_field_and_literal_tables() {
        let file = bash("echo \"${x}\" \"plain\" 7 $y\n");
        let roles: Vec<(&str, TokenRole)> = file
            .tokens
            .iter()
            .map(|token| (token.text.as_str(), token.role))
            .collect();
        assert!(roles.contains(&("echo", TokenRole::Callee)), "{roles:?}");
        assert!(roles.contains(&(
            "\"${x}\"",
            TokenRole::Literal(LiteralCategory::InterpolatedString)
        )));
        assert!(roles.contains(&("\"plain\"", TokenRole::Literal(LiteralCategory::Other))));
        assert!(roles.contains(&("7", TokenRole::Literal(LiteralCategory::Number))));
        assert!(roles.contains(&("y", TokenRole::Parameter)));
        assert!(file
            .tokens
            .iter()
            .filter(|token| !matches!(token.kind, TokenKind::Identifier | TokenKind::Literal))
            .all(|token| token.role == TokenRole::Unclassified));
    }

    /// `$y` is a `variable_name` inside a `simple_expansion`: as a wrapper,
    /// the expansion passes the lookup to its parent, but only to entries
    /// without a field, since the wrapper's own field is not known.
    #[test]
    fn role_wrappers_resolve_against_the_grandparent_with_any_field_only() {
        let wrapped = TokenClasses {
            kind_roles: &[],
            parent_roles: &[
                ("command_name", "", TokenRole::Callee),
                ("command", "", TokenRole::Member),
                ("variable_assignment", "value", TokenRole::Type),
            ],
            role_wrappers: &["simple_expansion"],
            ..BASH
        };
        let source = "echo $y\nx=$z\n";
        let roles_with = |classes: &TokenClasses| -> Vec<(String, TokenRole)> {
            tokenize_source(&tree_sitter_bash::LANGUAGE.into(), classes, "t.sh", source)
                .unwrap()
                .tokens
                .into_iter()
                .filter(|token| token.kind == TokenKind::Identifier)
                .map(|token| (token.text, token.role))
                .collect()
        };
        let roles = roles_with(&wrapped);
        for expected in [
            ("echo", TokenRole::Callee),
            ("y", TokenRole::Member),
            // The assignment entry needs the `value` field: not through a wrapper.
            ("z", TokenRole::Unclassified),
            ("x", TokenRole::Unclassified),
        ] {
            let expected = (expected.0.to_string(), expected.1);
            assert!(roles.contains(&expected), "{expected:?} in {roles:?}");
        }
        let unwrapped = roles_with(&TokenClasses {
            role_wrappers: &[],
            ..wrapped
        });
        assert!(
            unwrapped.contains(&("y".to_string(), TokenRole::Unclassified)),
            "{unwrapped:?}"
        );
    }

    /// Bash comments stand in for decorators and `if` for an enclosing
    /// construct; only the attachment and nesting mechanics are under test.
    #[test]
    fn callables_carry_attached_decorators_and_enclosing_labels() {
        let classes = TokenClasses {
            callable_kinds: &["function_definition"],
            decorator_kinds: &["comment"],
            context_kinds: &[("if_statement", "if")],
            ..BASH
        };
        let source = "\
# around   it
if true; then
  # first
  # second
  f() { echo; }
fi
# lost
echo x
g() { echo; }
";
        let file =
            tokenize_source(&tree_sitter_bash::LANGUAGE.into(), &classes, "t.sh", source).unwrap();
        let spans = file.callables.unwrap();
        let summary: Vec<(&str, &[String], &[String])> = spans
            .iter()
            .map(|span| {
                (
                    span.name.as_str(),
                    span.decorators.as_slice(),
                    span.enclosing.as_slice(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "f",
                    &["# first".to_string(), "# second".to_string()][..],
                    &["# around it if".to_string()][..]
                ),
                ("g", &[][..], &[][..]),
            ]
        );
        // Decorator kinds still go through the ordinary token rules.
        assert!(file.tokens.iter().all(|token| !token.text.starts_with('#')));
    }

    /// Bash commands stand in for calls, a command substitution for a non-flat
    /// argument and a `{ }` group for a statement wrapper; only the counting
    /// mechanics are under test.
    #[test]
    fn callables_report_how_many_body_statements_are_flat_calls() {
        let classes = TokenClasses {
            callable_kinds: &["function_definition"],
            call_kinds: &["command"],
            non_flat_kinds: &["command_substitution"],
            ..BASH
        };
        let source = "\
f() {
  # a comment is not a statement
  echo a
  echo \"$x\" b
}
g() {
  echo a
  echo $(date)
  x=1
}
h() { :; }
";
        let file =
            tokenize_source(&tree_sitter_bash::LANGUAGE.into(), &classes, "t.sh", source).unwrap();
        let shapes: Vec<(&str, Option<BodyShape>)> = file
            .callables
            .as_ref()
            .unwrap()
            .iter()
            .map(|span| (span.name.as_str(), span.body))
            .collect();
        let shape = |statements, flat_calls| {
            Some(BodyShape {
                statements,
                flat_calls,
            })
        };
        assert_eq!(
            shapes,
            [("f", shape(2, 2)), ("g", shape(3, 1)), ("h", shape(1, 1))]
        );
        assert!(shapes[0].1.unwrap().is_call_only());
        assert!(!shapes[1].1.unwrap().is_call_only());
        assert!(!BodyShape::default().is_call_only());

        // Without call kinds no shape is reported.
        let without = TokenClasses {
            callable_kinds: &["function_definition"],
            ..BASH
        };
        let file =
            tokenize_source(&tree_sitter_bash::LANGUAGE.into(), &without, "t.sh", source).unwrap();
        assert!(file
            .callables
            .unwrap()
            .iter()
            .all(|span| span.body.is_none()));
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
