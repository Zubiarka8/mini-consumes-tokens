//! String literals worth indexing, and the pure "prose" filter every
//! `mct-lang-*` crate runs them through before handing them to the indexer.
//!
//! Only human-readable messages are kept (error/log text, UI strings): those
//! are what an agent pastes into an exact-phrase search. Keys, paths, format
//! specs and secrets are dropped, which also keeps the index small.

use std::collections::HashSet;

/// Longest literal text kept, in chars; the rest is cut off.
pub const MAX_LITERAL_CHARS: usize = 256;

/// Shortest literal text kept, in chars, after trimming.
pub const MIN_LITERAL_CHARS: usize = 8;

/// Most literals kept per file. Bounds memory and index size against
/// generated or adversarial input (a file of a million strings).
pub const MAX_LITERALS_PER_FILE: usize = 500;

/// A token at least this long, with a digit and high character entropy, is
/// taken for a key/token/hash and drops its whole literal.
const SECRET_MIN_TOKEN_CHARS: usize = 20;

/// Shannon entropy, in bits per char, from which a long token counts as
/// random. English words and identifiers sit well below it; hex digests
/// and base64 keys above.
const SECRET_MIN_ENTROPY: f64 = 3.5;

/// One indexable fixed piece of a string literal. For an interpolated
/// string (f-string, template literal, `format!` spec) each fixed fragment
/// between two holes is its own `StringLiteral`, so a phrase never matches
/// across an interpolated value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringLiteral {
    /// The fragment's text: trimmed, whitespace runs collapsed to one space,
    /// at most [`MAX_LITERAL_CHARS`] chars.
    pub text: String,
    /// 1-based line the fragment starts on.
    pub line: u32,
}

/// `raw` normalised for indexing, or `None` when it isn't prose: fewer than
/// two words with a letter, shorter than [`MIN_LITERAL_CHARS`], or holding a
/// token that looks like a secret. Kept text is truncated to
/// [`MAX_LITERAL_CHARS`].
pub fn prose_literal(raw: &str) -> Option<String> {
    // Collapse whitespace and truncate before any other work, so a huge
    // literal costs no more than a short one.
    let mut text = String::new();
    let mut chars = 0;
    for word in raw.split_whitespace() {
        let extra = usize::from(!text.is_empty());
        if chars + extra >= MAX_LITERAL_CHARS {
            break;
        }
        if extra == 1 {
            text.push(' ');
            chars += 1;
        }
        for c in word.chars().take(MAX_LITERAL_CHARS - chars) {
            text.push(c);
            chars += 1;
        }
    }
    if chars < MIN_LITERAL_CHARS {
        return None;
    }
    let words = text
        .split(' ')
        .filter(|w| w.chars().any(char::is_alphabetic))
        .count();
    if words < 2 || text.split(' ').any(looks_like_secret) {
        return None;
    }
    Some(text)
}

/// Whether `token` looks like a key, token or hash rather than a word.
fn looks_like_secret(token: &str) -> bool {
    let len = token.chars().count();
    len >= SECRET_MIN_TOKEN_CHARS
        && token.chars().any(|c| c.is_ascii_digit())
        && shannon_entropy(token, len) >= SECRET_MIN_ENTROPY
}

fn shannon_entropy(token: &str, len: usize) -> f64 {
    let mut counts: Vec<(char, usize)> = Vec::new();
    for c in token.chars() {
        match counts.iter_mut().find(|(seen, _)| *seen == c) {
            Some((_, n)) => *n += 1,
            None => counts.push((c, 1)),
        }
    }
    let len = len as f64;
    counts
        .iter()
        .map(|&(_, n)| {
            let p = n as f64 / len;
            -p * p.log2()
        })
        .sum()
}

/// Collects a file's literals as a parser walks it: runs each through
/// [`prose_literal`], drops repeats of the same text, and stops accepting
/// after [`MAX_LITERALS_PER_FILE`].
#[derive(Debug, Default)]
pub struct LiteralCollector {
    literals: Vec<StringLiteral>,
    seen: HashSet<String>,
}

impl LiteralCollector {
    pub fn push(&mut self, raw: &str, line: u32) {
        if self.literals.len() >= MAX_LITERALS_PER_FILE {
            return;
        }
        let Some(text) = prose_literal(raw) else {
            return;
        };
        if self.seen.insert(text.clone()) {
            self.literals.push(StringLiteral { text, line });
        }
    }

    pub fn finish(self) -> Vec<StringLiteral> {
        self.literals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_messages_and_normalises_whitespace() {
        assert_eq!(
            prose_literal("  Error de conexión\n   con la BD: "),
            Some("Error de conexión con la BD:".to_string())
        );
        assert_eq!(
            prose_literal("failed to open index"),
            Some("failed to open index".to_string())
        );
    }

    #[test]
    fn drops_single_words_short_text_and_non_words() {
        assert_eq!(prose_literal("relative_path"), None);
        assert_eq!(prose_literal("src/main.rs"), None);
        assert_eq!(prose_literal("a b"), None);
        assert_eq!(prose_literal("{} {}"), None);
        assert_eq!(prose_literal("1 2 3 4 5 6"), None);
        assert_eq!(prose_literal("    "), None);
        assert_eq!(prose_literal(""), None);
    }

    #[test]
    fn drops_literals_holding_a_secret() {
        assert_eq!(prose_literal("Xk9mQ2vR7tLp4wZs8bNc3hJd"), None);
        assert_eq!(
            prose_literal("Authorization: Bearer 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b"),
            None
        );
        // Long plain words are not secrets.
        assert!(prose_literal("internationalization configuration failed").is_some());
    }

    #[test]
    fn truncates_on_a_char_boundary() {
        let long = "ñandú ".repeat(200);
        let text = prose_literal(&long).unwrap_or_default();
        assert_eq!(text.chars().count(), MAX_LITERAL_CHARS);
    }

    #[test]
    fn collector_dedupes_and_caps() {
        let mut collector = LiteralCollector::default();
        collector.push("could not parse the file", 3);
        collector.push("could not   parse the file", 9);
        collector.push("x", 10);
        let literals = collector.finish();
        assert_eq!(literals.len(), 1);
        assert_eq!(literals[0].line, 3);

        let mut collector = LiteralCollector::default();
        for i in 0..MAX_LITERALS_PER_FILE + 50 {
            collector.push(&format!("message number {i}"), 1);
        }
        assert_eq!(collector.finish().len(), MAX_LITERALS_PER_FILE);
    }
}
