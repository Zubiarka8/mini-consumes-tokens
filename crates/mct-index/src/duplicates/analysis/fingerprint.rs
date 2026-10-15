//! Turning a candidate's tokens into a comparable, normalized sequence.

use std::collections::HashMap;

use mct_core::{Token, TokenKind};

/// Id of every identifier after normalization.
const IDENTIFIER: usize = 0;
/// Id of every literal after normalization.
const LITERAL: usize = 1;
/// First id given to a keyword or punctuation text.
const FIRST_TEXT_ID: usize = 2;

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

/// Normalizes the tokens of lines `start..=end`: keywords and punctuation keep
/// their text, identifiers become one id and literals another.
///
/// `line_count` is the number of distinct lines covered by the tokens; a
/// token spanning several lines (a multi-line string) counts every line it
/// spans, because each of those lines holds token text.
pub(super) fn fingerprint(
    tokens: &[Token],
    start: u32,
    end: u32,
    interner: &mut Interner,
) -> Fingerprint {
    let mut sequence = Vec::new();
    let mut line_count = 0usize;
    // Last line already counted; tokens come in start order, so the covered
    // lines are the union of their spans, counted once each.
    let mut covered_to = 0u32;
    for token in within(tokens, start, end) {
        sequence.push(interner.id(token));
        let from = token.line.max(covered_to.saturating_add(1));
        if token.end_line >= from {
            line_count += usize::try_from(token.end_line - from + 1).unwrap_or(usize::MAX);
        }
        covered_to = covered_to.max(token.end_line);
    }
    Fingerprint {
        token_count: sequence.len(),
        sequence,
        line_count,
    }
}
