//! The approximate token counter shared by the token-reduction tests
//! (`tests/batch.rs`, `tests/context_pack.rs`, `tests/intent_eval.rs`),
//! `examples/format_benchmark.rs` and `examples/catalog_report.rs`.
//!
//! Each consumer includes this file with `#[path]` instead of going through
//! `common/mod.rs`, so none of them compiles the large retrieval fixtures.

/// A crude token-count approximation: every run of alphanumeric/`_`
/// characters counts as one token, and every other non-whitespace character
/// (punctuation — `{`, `"`, `,`, `:`, `[`, `]`, `-`, `>` ...) counts as its
/// own token. This mirrors how BPE tokenizers tend to split JSON/TOON
/// punctuation into single-character tokens while keeping identifiers
/// mostly whole, without pulling in a real tokenizer dependency.
pub fn approx_tokens(s: &str) -> usize {
    let mut tokens = 0usize;
    let mut in_word = false;
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' {
            if !in_word {
                tokens += 1;
                in_word = true;
            }
        } else {
            in_word = false;
            if !c.is_whitespace() {
                tokens += 1;
            }
        }
    }
    tokens
}
