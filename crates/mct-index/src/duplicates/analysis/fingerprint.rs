//! Turning a candidate's tokens into a comparable, normalized sequence.

use std::collections::HashMap;

use mct_core::{Token, TokenKind};

use super::evidence::{Slot, Texts};

/// Id of every identifier after normalization.
const IDENTIFIER: usize = 0;
/// Id of every literal after normalization.
const LITERAL: usize = 1;
/// Id of every synthetic block start.
const BLOCK_START: usize = 2;
/// Id of every synthetic block end.
const BLOCK_END: usize = 3;
/// First id given to a keyword or punctuation text.
const FIRST_TEXT_ID: usize = 4;

/// Interns token categories separately; hash collisions never imply equality.
/// Also holds the original texts that aligned evidence refers to.
#[derive(Default)]
pub(super) struct Interner {
    text: [HashMap<String, usize>; 4],
    count: usize,
    pub(super) texts: Texts,
}

impl Interner {
    fn id(&mut self, kind: TokenKind, text: &str) -> usize {
        // The category is a closed enum: direct selection avoids a second
        // hash-table lookup for every source token.
        let category = match kind {
            TokenKind::Identifier => 0,
            TokenKind::Literal => 1,
            TokenKind::Keyword => 2,
            TokenKind::Punctuation => 3,
            TokenKind::BlockStart => return BLOCK_START,
            TokenKind::BlockEnd => return BLOCK_END,
        };
        let table = &mut self.text[category];
        if let Some(id) = table.get(text) {
            return *id;
        }
        let next = FIRST_TEXT_ID + self.count;
        self.count += 1;
        table.insert(text.to_string(), next);
        next
    }
}

/// A candidate's tokens, measured and normalized.
pub(super) struct Fingerprint {
    pub(super) sequence: Vec<usize>,
    pub(super) token_count: usize,
    pub(super) line_count: usize,
    /// The original identifier and literal tokens, in sequence order, for
    /// aligned evidence once the candidate is part of a group.
    pub(super) slots: Vec<Slot>,
}

/// The tokens of `tokens` (sorted by `(line, start_byte)`) lying entirely
/// within lines `start..=end`.
fn within(tokens: &[Token], start: u32, end: u32) -> impl Iterator<Item = &Token> {
    let first = tokens.partition_point(|token| token.line < start);
    tokens
        .iter()
        .skip(first)
        .take_while(move |token| token.line <= end)
        .filter(move |token| token.end_line <= end)
}

/// Which of `tokens` (a candidate's, in order) to keep: every source token,
/// and only the block markers of blocks that lie wholly inside the candidate.
///
/// Lines alone cannot tell a candidate's markers from those of a block that
/// encloses it: a class body starts on its first method's first line and ends
/// on its last method's last line. A marker before the first source token
/// cannot be the candidate's own (its blocks open after a header), and of the
/// rest only start/end pairs that match each other are its own.
fn own_markers(tokens: &[&Token]) -> Vec<bool> {
    let mut keep = vec![false; tokens.len()];
    let Some(first) = tokens.iter().position(|t| !t.kind.is_block_marker()) else {
        return keep;
    };
    let mut open = Vec::new();
    for (i, token) in tokens.iter().enumerate().skip(first) {
        match token.kind {
            TokenKind::BlockStart => open.push(i),
            TokenKind::BlockEnd => {
                if let Some(start) = open.pop() {
                    keep[start] = true;
                    keep[i] = true;
                }
            }
            _ => keep[i] = true,
        }
    }
    keep
}

/// The candidate's tokens as a partial-block stream: the same selection and
/// markers as [`fingerprint`], but identifiers are never renumbered. Under
/// `broad` and `consistent` every identifier is one id and every literal
/// another; `consistent` also keeps each identifier's spelling, so blocks can
/// be split by identifier pattern over the block rather than the function.
/// `exact` keeps all text, as for whole functions.
pub(super) fn block_tokens(
    tokens: &[Token],
    start: u32,
    end: u32,
    bytes: Option<&std::ops::Range<usize>>,
    interner: &mut Interner,
    normalization: super::super::Normalization,
) -> Vec<super::blocks::BlockToken> {
    use super::super::Normalization;
    let selected: Vec<&Token> = within(tokens, start, end)
        .filter(|token| {
            bytes.is_none_or(|range| token.start_byte >= range.start && token.end_byte <= range.end)
        })
        .collect();
    let keep = own_markers(&selected);
    let mut stream = Vec::with_capacity(selected.len());
    for (token, keep) in selected.into_iter().zip(keep) {
        if !keep {
            continue;
        }
        let id = match token.kind {
            TokenKind::BlockStart => BLOCK_START,
            TokenKind::BlockEnd => BLOCK_END,
            TokenKind::Identifier if normalization != Normalization::Exact => IDENTIFIER,
            TokenKind::Literal if normalization != Normalization::Exact => LITERAL,
            _ => interner.id(token.kind, &token.text),
        };
        let spelling =
            if normalization == Normalization::Consistent && token.kind == TokenKind::Identifier {
                u32::try_from(interner.id(TokenKind::Identifier, &token.text))
                    .unwrap_or(super::blocks::NO_SPELLING - 1)
            } else {
                super::blocks::NO_SPELLING
            };
        let bracket = match token.kind {
            TokenKind::BlockStart => 1,
            TokenKind::BlockEnd => -1,
            TokenKind::Punctuation => match token.text.as_str() {
                "(" | "[" | "{" => 1,
                ")" | "]" | "}" => -1,
                _ => 0,
            },
            _ => 0,
        };
        stream.push(super::blocks::BlockToken {
            id: u64::try_from(id).unwrap_or(u64::MAX),
            spelling,
            marker: token.kind.is_block_marker(),
            bracket,
            line: token.line,
            end_line: token.end_line,
            line_first: false,
            line_last: false,
        });
    }
    super::blocks::mark_lines(&mut stream);
    stream
}

/// Selects tokens inside the optional byte boundary and line range, then
/// applies the requested identifier/literal policy. Keywords, punctuation and
/// the candidate's paired block markers retain their structure.
///
/// `line_count` is the number of distinct lines covered by the tokens; a
/// token spanning several lines (a multi-line string) counts every line it
/// spans, because each of those lines holds token text. Block markers are
/// part of the sequence but not source text: they add to neither count.
pub(super) fn fingerprint(
    tokens: &[Token],
    start: u32,
    end: u32,
    bytes: Option<&std::ops::Range<usize>>,
    interner: &mut Interner,
    normalization: super::super::Normalization,
    name: &str,
) -> Fingerprint {
    let selected: Vec<&Token> = within(tokens, start, end)
        .filter(|token| {
            bytes.is_none_or(|range| token.start_byte >= range.start && token.end_byte <= range.end)
        })
        .collect();
    let keep = own_markers(&selected);
    let mut names = HashMap::new();
    if normalization == super::super::Normalization::Consistent {
        names.insert(name.to_string(), 0usize);
    }
    let mut sequence = Vec::new();
    let mut slots = Vec::new();
    let mut token_count = 0usize;
    let mut line_count = 0usize;
    // Last line already counted; tokens come in start order, so the covered
    // lines are the union of their spans, counted once each.
    let mut covered_to = 0u32;
    for (token, keep) in selected.into_iter().zip(keep) {
        if !keep {
            continue;
        }
        use super::super::Normalization;
        let id = match token.kind {
            TokenKind::BlockStart => BLOCK_START,
            TokenKind::BlockEnd => BLOCK_END,
            TokenKind::Identifier if normalization == Normalization::Broad => IDENTIFIER,
            TokenKind::Identifier if normalization == Normalization::Consistent => {
                let next = names.len();
                let local = *names.entry(token.text.clone()).or_insert(next);
                interner.id(TokenKind::Identifier, &local.to_string())
            }
            TokenKind::Literal if normalization != Normalization::Exact => LITERAL,
            _ => interner.id(token.kind, &token.text),
        };
        sequence.push(id);
        if token.kind.is_block_marker() {
            continue;
        }
        if matches!(token.kind, TokenKind::Identifier | TokenKind::Literal) {
            slots.push(Slot {
                literal: token.kind == TokenKind::Literal,
                role: token.role,
                primitive_spelling: token.primitive_spelling,
                text: interner.texts.id(&token.text),
                line: token.line,
            });
        }
        token_count += 1;
        let from = token.line.max(covered_to.saturating_add(1));
        if token.end_line >= from {
            line_count += usize::try_from(token.end_line - from + 1).unwrap_or(usize::MAX);
        }
        covered_to = covered_to.max(token.end_line);
    }
    Fingerprint {
        token_count,
        sequence,
        line_count,
        slots,
    }
}
