//! Turning a candidate's tokens into a comparable, normalized sequence.

use std::collections::HashMap;

use mct_core::{Token, TokenKind};

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

/// Maps the text of keywords and punctuation to small ids, so a candidate's
/// normalized sequence is a `Vec<usize>` compared exactly (no hash-only
/// equality) without keeping a string per token. Keyword and punctuation ids
/// never collide: the same text in two kinds gets two ids.
#[derive(Default)]
pub(super) struct Interner {
    keywords: HashMap<String, usize>,
    punctuation: HashMap<String, usize>,
}

impl Interner {
    fn id(&mut self, token: &Token) -> usize {
        // One id space across both tables: the next unused id is the count of
        // ids handed out so far.
        let next = FIRST_TEXT_ID + self.keywords.len() + self.punctuation.len();
        let table = match token.kind {
            TokenKind::Identifier => return IDENTIFIER,
            TokenKind::Literal => return LITERAL,
            TokenKind::BlockStart => return BLOCK_START,
            TokenKind::BlockEnd => return BLOCK_END,
            TokenKind::Keyword => &mut self.keywords,
            TokenKind::Punctuation => &mut self.punctuation,
        };
        if let Some(id) = table.get(&token.text) {
            return *id;
        }
        table.insert(token.text.clone(), next);
        next
    }
}

/// A candidate's tokens, measured and normalized.
pub(super) struct Fingerprint {
    pub(super) sequence: Vec<usize>,
    pub(super) token_count: usize,
    pub(super) line_count: usize,
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

/// Normalizes the tokens of lines `start..=end`: keywords and punctuation keep
/// their text, identifiers become one id, literals another, and the
/// candidate's own block markers one id per side (see [`own_markers`]).
///
/// `line_count` is the number of distinct lines covered by the tokens; a
/// token spanning several lines (a multi-line string) counts every line it
/// spans, because each of those lines holds token text. Block markers are
/// part of the sequence but not source text: they add to neither count.
pub(super) fn fingerprint(
    tokens: &[Token],
    start: u32,
    end: u32,
    interner: &mut Interner,
) -> Fingerprint {
    let selected: Vec<&Token> = within(tokens, start, end).collect();
    let keep = own_markers(&selected);
    let mut sequence = Vec::new();
    let mut token_count = 0usize;
    let mut line_count = 0usize;
    // Last line already counted; tokens come in start order, so the covered
    // lines are the union of their spans, counted once each.
    let mut covered_to = 0u32;
    for (token, keep) in selected.into_iter().zip(keep) {
        if !keep {
            continue;
        }
        sequence.push(interner.id(token));
        if token.kind.is_block_marker() {
            continue;
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
    }
}
