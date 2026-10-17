//! Repeated partial blocks: token sequences shared by two or more different
//! functions, below the whole-function level (issue #207, Phase 3).
//!
//! Pure over its inputs: one normalized token stream per outermost function,
//! already in a deterministic order. Nothing here reads a file or the index.
//!
//! Algorithm:
//! 1. Hash every window of `w` tokens (`w = min(min_tokens, MAX_WINDOW)`) with
//!    a polynomial rolling hash. A block of at least `min_tokens` tokens
//!    starts with such a window, so the windows are a complete seed set.
//! 2. Sort the windows by hash, then split each equal-hash run into classes by
//!    comparing the windows token by token: a hash collision never merges two
//!    different sequences.
//! 3. Walk each class like a suffix-tree node: extend all positions to the
//!    right while they agree, and split them by the next token when they stop
//!    agreeing. Every node is a maximal repeat for its set of positions. A set
//!    whose positions all share their preceding token is skipped, because the
//!    class one token to the left reports it with a longer extent.
//! 4. Keep the first occurrence per function (repetition inside one function
//!    is not reported), trim each node to whole lines and balanced delimiters
//!    (block markers count as delimiters), and apply the thresholds.
//! 5. Drop blocks whose every occurrence lies inside members of one reported
//!    whole-function group, then keep a block only if at least one of its
//!    occurrences is not inside an occurrence of a larger kept block.
//!
//! Budgets are deterministic counts, checked before the work they bound.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::super::{BlockBudgetKind, PartialBlockBudget};

/// `spelling` of a token that is not an identifier.
pub(super) const NO_SPELLING: u32 = u32::MAX;
/// Longest window hashed. Longer thresholds still find every block, because
/// a window is only a seed; capping it bounds the cost of verifying a window.
const MAX_WINDOW: usize = 64;
/// Multiplier of the rolling hash (odd, so invertible modulo 2^64).
const BASE: u64 = 0x0000_0100_0000_01b3;

/// One token of a stream: its normalized id plus what trimming needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct BlockToken {
    /// Normalized id; equal ids mean equal tokens under the policy in use.
    pub(super) id: u64,
    /// Interned identifier spelling for the consistent policy, or
    /// [`NO_SPELLING`].
    pub(super) spelling: u32,
    /// A synthetic block marker: not source text, counts as no token.
    pub(super) marker: bool,
    /// `1` opens a delimiter (`(`, `[`, `{`, a block start), `-1` closes one.
    pub(super) bracket: i8,
    pub(super) line: u32,
    pub(super) end_line: u32,
    /// First source token on its line within the stream.
    pub(super) line_first: bool,
    /// Last source token on its line within the stream.
    pub(super) line_last: bool,
}

/// Sets [`BlockToken::line_first`] and [`BlockToken::line_last`] from the
/// lines of the stream's source tokens; markers get neither.
pub(super) fn mark_lines(tokens: &mut [BlockToken]) {
    let mut previous_end: Option<u32> = None;
    for token in tokens.iter_mut() {
        if token.marker {
            continue;
        }
        token.line_first = previous_end.is_none_or(|end| end < token.line);
        previous_end = Some(token.end_line);
    }
    let mut next_line: Option<u32> = None;
    for token in tokens.iter_mut().rev() {
        if token.marker {
            continue;
        }
        token.line_last = next_line.is_none_or(|line| line > token.end_line);
        next_line = Some(token.line);
    }
}

/// One function's tokens; streams of different languages never match.
pub(super) struct Stream {
    pub(super) language: u32,
    pub(super) tokens: Vec<BlockToken>,
}

/// A run of tokens `start..end` (positions, end exclusive) of one stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct Span {
    pub(super) stream: usize,
    pub(super) start: usize,
    pub(super) end: usize,
}

/// A repeated block before ids are assigned. `spans` holds one span per
/// stream, ordered by stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RawBlock {
    pub(super) spans: Vec<Span>,
    pub(super) tokens: usize,
    pub(super) lines: usize,
}

/// What a search found and what it cost.
#[derive(Debug, Default)]
pub(super) struct Outcome {
    pub(super) blocks: Vec<RawBlock>,
    pub(super) functions_searched: usize,
    pub(super) windows_used: usize,
    pub(super) comparisons_used: usize,
    pub(super) exhausted: Option<BlockBudgetKind>,
}

struct Search<'a> {
    streams: &'a [Stream],
    limits: PartialBlockBudget,
    min_tokens: usize,
    min_lines: usize,
    consistent: bool,
    comparisons: usize,
    exhausted: Option<BlockBudgetKind>,
    found: BTreeSet<Vec<Span>>,
    blocks: Vec<RawBlock>,
}

/// Finds repeated blocks across `streams` (ordered as the report should
/// break ties), drops those `covered` says lie inside one whole-function
/// group, and keeps maximal, non-redundant blocks in report order.
/// `hash_mask` is `u64::MAX` in production; tests pass `0` to force every
/// window into one hash bucket.
pub(super) fn find_blocks(
    streams: &[Stream],
    limits: PartialBlockBudget,
    consistent: bool,
    covered: impl Fn(&[Span]) -> bool,
    hash_mask: u64,
) -> Outcome {
    let min_tokens = limits.min_tokens.max(1);
    let min_lines = limits.min_lines.max(1);
    let window = min_tokens.min(MAX_WINDOW);
    let mut outcome = Outcome::default();

    // 1. Windows of every eligible stream, until the window budget runs out.
    let mut entries: Vec<(u32, u64, u32, u32)> = Vec::new();
    for (index, stream) in streams.iter().enumerate() {
        let tokens = &stream.tokens;
        if tokens.len() < window
            || source_tokens(tokens) < min_tokens
            || line_count(tokens) < min_lines
        {
            continue;
        }
        let count = tokens.len() - window + 1;
        if outcome.windows_used.saturating_add(count) > limits.max_windows {
            outcome.exhausted = Some(BlockBudgetKind::MaxWindows);
            break;
        }
        let (Ok(stream_id), Ok(_)) = (u32::try_from(index), u32::try_from(tokens.len())) else {
            continue;
        };
        outcome.windows_used += count;
        outcome.functions_searched += 1;
        for (position, hash) in window_hashes(tokens, window).into_iter().enumerate() {
            // Positions fit in u32: the stream length was checked above.
            let position = u32::try_from(position).unwrap_or(u32::MAX);
            entries.push((stream.language, hash & hash_mask, stream_id, position));
        }
    }
    entries.sort_unstable();

    // 2. Verified classes: equal language, equal hash and equal tokens.
    let mut classes: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut run_start = 0;
    while run_start < entries.len() {
        let key = (entries[run_start].0, entries[run_start].1);
        let run_end = entries[run_start..]
            .iter()
            .position(|entry| (entry.0, entry.1) != key)
            .map_or(entries.len(), |offset| run_start + offset);
        let mut verified: Vec<Vec<(usize, usize)>> = Vec::new();
        for &(_, _, stream, position) in &entries[run_start..run_end] {
            let at = (stream as usize, position as usize);
            let slice = |(s, p): (usize, usize)| &streams[s].tokens[p..p + window];
            match verified
                .iter_mut()
                .find(|class| same_ids(slice(class[0]), slice(at)))
            {
                Some(class) => class.push(at),
                None => verified.push(vec![at]),
            }
        }
        classes.extend(
            verified
                .into_iter()
                .filter(|class| distinct_streams(class) >= 2),
        );
        run_start = run_end;
    }
    drop(entries);
    classes.sort_unstable_by_key(|class| class[0]);

    // 3-4. Walk each class; nodes become trimmed, thresholded blocks.
    let mut search = Search {
        streams,
        limits,
        min_tokens,
        min_lines,
        consistent,
        comparisons: 0,
        exhausted: None,
        found: BTreeSet::new(),
        blocks: Vec::new(),
    };
    for class in classes {
        if search.exhausted.is_some() {
            break;
        }
        if search.left_maximal(&class) {
            search.walk(class, window);
        }
    }
    outcome.comparisons_used = search.comparisons;
    if outcome.exhausted.is_none() {
        outcome.exhausted = search.exhausted;
    }

    // 5. Exclusion, report order, maximality and the block budget.
    let mut blocks: Vec<RawBlock> = search
        .blocks
        .into_iter()
        .filter(|block| !covered(&block.spans))
        .collect();
    blocks.sort_by(|a, b| {
        b.tokens
            .cmp(&a.tokens)
            .then_with(|| b.spans.len().cmp(&a.spans.len()))
            .then_with(|| a.spans.cmp(&b.spans))
    });
    let mut kept: Vec<RawBlock> = Vec::new();
    let mut kept_spans: HashMap<usize, Vec<Span>> = HashMap::new();
    for block in blocks {
        // Overlap, not only containment: a periodic run (a table of similar
        // rows) otherwise yields the same block again at every shift.
        let redundant = block.spans.iter().all(|span| {
            kept_spans.get(&span.stream).is_some_and(|spans| {
                spans
                    .iter()
                    .any(|kept| kept.start < span.end && span.start < kept.end)
            })
        });
        if redundant {
            continue;
        }
        for span in &block.spans {
            kept_spans.entry(span.stream).or_default().push(*span);
        }
        kept.push(block);
    }
    if kept.len() > limits.max_blocks {
        kept.truncate(limits.max_blocks);
        if outcome.exhausted.is_none() {
            outcome.exhausted = Some(BlockBudgetKind::MaxBlocks);
        }
    }
    outcome.blocks = kept;
    outcome
}

impl Search<'_> {
    fn token(&self, (stream, position): (usize, usize)) -> Option<&BlockToken> {
        self.streams[stream].tokens.get(position)
    }

    /// False when every position has the same preceding token: the class
    /// one token to the left holds these positions with a longer extent.
    fn left_maximal(&self, positions: &[(usize, usize)]) -> bool {
        let previous = |&(stream, position): &(usize, usize)| {
            position
                .checked_sub(1)
                .and_then(|before| self.token((stream, before)))
                .map(|token| token.id)
        };
        let Some(first) = positions.first().and_then(previous) else {
            return true;
        };
        positions
            .iter()
            .any(|position| previous(position) != Some(first))
    }

    /// Charges `cost` comparisons; false (and the budget marked exhausted)
    /// when that would exceed the comparison budget.
    fn charge(&mut self, cost: usize) -> bool {
        if self.comparisons.saturating_add(cost) > self.limits.max_comparisons {
            self.exhausted = Some(BlockBudgetKind::MaxComparisons);
            return false;
        }
        self.comparisons += cost;
        true
    }

    /// Depth-first walk from a class whose positions agree on `length` ids.
    fn walk(&mut self, class: Vec<(usize, usize)>, length: usize) {
        let mut stack = vec![(class, length)];
        while let Some((positions, mut length)) = stack.pop() {
            // Extend while every position has an equal next token.
            loop {
                if !self.charge(positions.len()) {
                    return;
                }
                let probe = length;
                let next = |at: &(usize, usize)| self.token((at.0, at.1 + probe)).map(|t| t.id);
                let first = next(&positions[0]);
                let agree = first.is_some() && positions.iter().all(|at| next(at) == first);
                if agree {
                    length += 1;
                } else {
                    break;
                }
            }
            self.node(&positions, length);
            let mut children: BTreeMap<u64, Vec<(usize, usize)>> = BTreeMap::new();
            for &at in &positions {
                if let Some(token) = self.token((at.0, at.1 + length)) {
                    children.entry(token.id).or_default().push(at);
                }
            }
            for child in children.into_values().rev() {
                if distinct_streams(&child) >= 2 && self.left_maximal(&child) {
                    stack.push((child, length + 1));
                }
            }
        }
    }

    /// Turns one maximal repeat into blocks: first occurrence per stream,
    /// trimmed, thresholded and, under the consistent policy, split by
    /// identifier pattern.
    fn node(&mut self, positions: &[(usize, usize)], length: usize) {
        let mut first: BTreeMap<usize, usize> = BTreeMap::new();
        for &(stream, position) in positions {
            let entry = first.entry(stream).or_insert(position);
            *entry = (*entry).min(position);
        }
        if first.len() < 2 {
            return;
        }
        let starts: Vec<(usize, usize)> = first.into_iter().collect();
        let Some((from, to)) = self.trim(&starts, length) else {
            return;
        };
        let (stream, position) = starts[0];
        let tokens = source_tokens(&self.streams[stream].tokens[position + from..position + to]);
        if tokens < self.min_tokens {
            return;
        }
        let spans: Vec<Span> = starts
            .iter()
            .map(|&(stream, position)| Span {
                stream,
                start: position + from,
                end: position + to,
            })
            .collect();
        let lines = spans
            .iter()
            .map(|span| line_count(&self.streams[span.stream].tokens[span.start..span.end]))
            .min()
            .unwrap_or(0);
        if lines < self.min_lines {
            return;
        }
        let sets: Vec<Vec<Span>> = if self.consistent {
            let mut by_pattern: BTreeMap<Vec<u32>, Vec<Span>> = BTreeMap::new();
            for span in spans {
                let pattern =
                    spelling_pattern(&self.streams[span.stream].tokens[span.start..span.end]);
                by_pattern.entry(pattern).or_default().push(span);
            }
            by_pattern
                .into_values()
                .filter(|set| set.len() >= 2)
                .collect()
        } else {
            vec![spans]
        };
        for spans in sets {
            let lines = spans
                .iter()
                .map(|span| line_count(&self.streams[span.stream].tokens[span.start..span.end]))
                .min()
                .unwrap_or(0);
            if self.found.insert(spans.clone()) {
                self.blocks.push(RawBlock {
                    spans,
                    tokens,
                    lines,
                });
            }
        }
    }

    /// Narrows offsets `0..length`, shared by every start, until the block
    /// starts on the first token of a line and ends on the last token of a
    /// line in every occurrence, and its delimiters balance. `None` when
    /// nothing is left.
    fn trim(&self, starts: &[(usize, usize)], length: usize) -> Option<(usize, usize)> {
        let at = |offset: usize, &(stream, position): &(usize, usize)| {
            self.streams[stream].tokens.get(position + offset)
        };
        let (mut from, mut to) = (0usize, length);
        loop {
            while from < to
                && !starts
                    .iter()
                    .all(|start| at(from, start).is_some_and(|t| !t.marker && t.line_first))
            {
                from += 1;
            }
            // The last source token must end a line; markers after it stay,
            // so a block that ends a layout block keeps its closing marker.
            loop {
                let last = (from..to)
                    .rev()
                    .find(|&offset| at(offset, &starts[0]).is_some_and(|token| !token.marker))?;
                if starts
                    .iter()
                    .all(|start| at(last, start).is_some_and(|t| t.line_last))
                {
                    break;
                }
                to = last;
            }
            if from >= to {
                return None;
            }
            let (stream, position) = starts[0];
            let tokens = &self.streams[stream].tokens[position + from..position + to];
            let (cut_start, cut_end) = longest_balanced(tokens)?;
            let (next_from, next_to) = (from + cut_start, from + cut_end);
            if (next_from, next_to) == (from, to) {
                return Some((from, to));
            }
            (from, to) = (next_from, next_to);
        }
    }
}

/// The balanced run of `tokens` with the most source tokens (the earliest on
/// a tie), as offsets `start..end`. Unmatched closers and unmatched openers
/// cut the run; every piece between two cuts is balanced, because a closer
/// always matches the innermost opener still open. `None` when every piece
/// is empty.
fn longest_balanced(tokens: &[BlockToken]) -> Option<(usize, usize)> {
    let mut cuts = Vec::new();
    let mut open = Vec::new();
    for (offset, token) in tokens.iter().enumerate() {
        match token.bracket {
            1 => open.push(offset),
            -1 if open.pop().is_none() => cuts.push(offset),
            _ => {}
        }
    }
    cuts.extend(open);
    cuts.sort_unstable();
    let mut best: Option<(usize, usize, usize)> = None;
    let mut start = 0usize;
    for end in cuts.into_iter().chain(std::iter::once(tokens.len())) {
        if end > start {
            let size = source_tokens(&tokens[start..end]);
            if size > 0 && best.is_none_or(|(_, _, most)| size > most) {
                best = Some((start, end, size));
            }
        }
        start = end + 1;
    }
    best.map(|(start, end, _)| (start, end))
}

fn same_ids(a: &[BlockToken], b: &[BlockToken]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.id == y.id)
}

fn distinct_streams(positions: &[(usize, usize)]) -> usize {
    let mut streams: Vec<usize> = positions.iter().map(|&(stream, _)| stream).collect();
    streams.sort_unstable();
    streams.dedup();
    streams.len()
}

/// Source tokens, block markers excluded.
pub(super) fn source_tokens(tokens: &[BlockToken]) -> usize {
    tokens.iter().filter(|token| !token.marker).count()
}

/// Distinct lines covered by source tokens; a multi-line token counts every
/// line it spans, as for whole functions.
pub(super) fn line_count(tokens: &[BlockToken]) -> usize {
    let mut count = 0usize;
    let mut covered_to = 0u32;
    for token in tokens.iter().filter(|token| !token.marker) {
        let from = token.line.max(covered_to.saturating_add(1));
        if token.end_line >= from {
            count += usize::try_from(token.end_line - from + 1).unwrap_or(usize::MAX);
        }
        covered_to = covered_to.max(token.end_line);
    }
    count
}

/// Identifier spellings renumbered by first appearance: equal patterns mean
/// a consistent one-to-one renaming between two occurrences.
fn spelling_pattern(tokens: &[BlockToken]) -> Vec<u32> {
    let mut numbers: HashMap<u32, u32> = HashMap::new();
    tokens
        .iter()
        .filter(|token| token.spelling != NO_SPELLING)
        .map(|token| {
            let next = u32::try_from(numbers.len()).unwrap_or(u32::MAX);
            *numbers.entry(token.spelling).or_insert(next)
        })
        .collect()
}

/// Rolling hash of every `window`-token run, by position.
fn window_hashes(tokens: &[BlockToken], window: usize) -> Vec<u64> {
    let mut prefix = Vec::with_capacity(tokens.len() + 1);
    prefix.push(0u64);
    let mut hash = 0u64;
    for token in tokens {
        hash = hash.wrapping_mul(BASE).wrapping_add(mix(token.id));
        prefix.push(hash);
    }
    let mut power = 1u64;
    for _ in 0..window {
        power = power.wrapping_mul(BASE);
    }
    (0..=tokens.len().saturating_sub(window))
        .map(|start| prefix[start + window].wrapping_sub(prefix[start].wrapping_mul(power)))
        .collect()
}

/// splitmix64 finalizer, so small consecutive ids spread over the hash.
fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// Builds streams from text: one line per source line, words separated by
    /// spaces. `$name` is an identifier (normalized id 0, spelling kept),
    /// `BS`/`BE` are block markers, brackets get their delimiter role.
    struct Corpus {
        ids: HashMap<String, u64>,
        spellings: HashMap<String, u32>,
    }

    impl Corpus {
        fn new() -> Self {
            Self {
                ids: HashMap::new(),
                spellings: HashMap::new(),
            }
        }

        fn stream(&mut self, text: &str) -> Stream {
            self.stream_in(0, text)
        }

        fn stream_in(&mut self, language: u32, text: &str) -> Stream {
            let mut tokens = Vec::new();
            for (number, line) in text.lines().enumerate() {
                let line_number = u32::try_from(number).unwrap() + 1;
                for word in line.split_whitespace() {
                    let marker = word == "BS" || word == "BE";
                    let identifier = word.starts_with('$');
                    let next = u64::try_from(self.ids.len()).unwrap() + 10;
                    let id = if identifier {
                        0
                    } else {
                        *self.ids.entry(word.to_string()).or_insert(next)
                    };
                    let next_spelling = u32::try_from(self.spellings.len()).unwrap();
                    let spelling = if identifier {
                        *self
                            .spellings
                            .entry(word.to_string())
                            .or_insert(next_spelling)
                    } else {
                        NO_SPELLING
                    };
                    tokens.push(BlockToken {
                        id,
                        spelling,
                        marker,
                        bracket: match word {
                            "(" | "[" | "{" | "BS" => 1,
                            ")" | "]" | "}" | "BE" => -1,
                            _ => 0,
                        },
                        line: line_number,
                        end_line: line_number,
                        line_first: false,
                        line_last: false,
                    });
                }
            }
            mark_lines(&mut tokens);
            Stream { language, tokens }
        }
    }

    fn budget(min_tokens: usize, min_lines: usize) -> PartialBlockBudget {
        PartialBlockBudget {
            min_tokens,
            min_lines,
            ..PartialBlockBudget::default()
        }
    }

    fn run(streams: &[Stream], limits: PartialBlockBudget) -> Outcome {
        find_blocks(streams, limits, false, |_| false, u64::MAX)
    }

    /// `(stream, first line, last line)` of every span, per block.
    fn located(streams: &[Stream], outcome: &Outcome) -> Vec<Vec<(usize, u32, u32)>> {
        outcome
            .blocks
            .iter()
            .map(|block| {
                block
                    .spans
                    .iter()
                    .map(|span| {
                        let tokens = &streams[span.stream].tokens;
                        (
                            span.stream,
                            tokens[span.start].line,
                            tokens[span.end - 1].end_line,
                        )
                    })
                    .collect()
            })
            .collect()
    }

    const SHARED: &str = "let $a = load ( $b ) ;\nif $a > 0 {\nsave ( $a , 1 ) ;\n}\n";

    #[test]
    fn a_block_shared_by_two_functions_is_found_with_its_exact_lines() {
        let mut corpus = Corpus::new();
        let streams = vec![
            corpus.stream(&format!("fn $f ( ) {{\nprint ( 1 ) ;\n{SHARED}}}\n")),
            corpus.stream(&format!("fn $g ( $x ) {{\n{SHARED}return $x * 2 ;\n}}\n")),
        ];
        let outcome = run(&streams, budget(10, 3));
        assert_eq!(located(&streams, &outcome), [[(0, 3, 6), (1, 2, 5)]]);
        assert_eq!(outcome.blocks[0].tokens, 21);
        assert_eq!(outcome.blocks[0].lines, 4);
        assert_eq!(outcome.exhausted, None);
        assert_eq!(outcome.functions_searched, 2);
    }

    #[test]
    fn blocks_below_either_minimum_are_not_reported() {
        let mut corpus = Corpus::new();
        let streams = vec![
            corpus.stream(&format!("fn $f ( ) {{\nprint ( 1 ) ;\n{SHARED}}}\n")),
            corpus.stream(&format!("fn $g ( $x ) {{\n{SHARED}return $x * 2 ;\n}}\n")),
        ];
        assert!(run(&streams, budget(22, 1)).blocks.is_empty());
        assert!(run(&streams, budget(5, 5)).blocks.is_empty());
        assert_eq!(run(&streams, budget(21, 4)).blocks.len(), 1);
    }

    #[test]
    fn a_long_block_is_one_maximal_block_not_its_windows() {
        let mut corpus = Corpus::new();
        let long = SHARED.to_string() + "while $a < 9 {\n$a = $a + 1 ;\n}\nemit ( $a ) ;\n";
        let streams = vec![
            corpus.stream(&format!("fn $f ( ) {{\nfirst ( ) ;\n{long}}}\n")),
            corpus.stream(&format!("fn $g ( ) {{\nsecond ( 2 ) ;\n{long}}}\n")),
        ];
        // A window of 4 tokens seeds the search; the block is extended to
        // the whole shared run and reported once. Two lines at least, so the
        // shared one-line `fn $ ( )` header is not a block of its own.
        let outcome = run(&streams, budget(4, 2));
        assert_eq!(located(&streams, &outcome), [[(0, 3, 10), (1, 3, 10)]]);
    }

    #[test]
    fn a_match_is_trimmed_to_whole_lines_and_balanced_delimiters() {
        let mut corpus = Corpus::new();
        // The shared run starts mid-line (`x ;`) and ends inside an open
        // brace; trimming keeps whole lines with balanced delimiters only.
        let streams = vec![
            corpus.stream(&format!("a b x ;\n{SHARED}while ( 1 ) {{\nz ;\n")),
            corpus.stream(&format!("c d x ;\n{SHARED}while ( 1 ) {{\ny ;\n")),
        ];
        let outcome = run(&streams, budget(4, 1));
        assert_eq!(located(&streams, &outcome), [[(0, 2, 5), (1, 2, 5)]]);
    }

    #[test]
    fn unmatched_block_markers_are_trimmed_like_brackets() {
        let mut corpus = Corpus::new();
        let body = "BS step ( 1 ) ;\nstep ( 2 ) ;\nstep ( 3 ) ; BE\n";
        let streams = vec![
            corpus.stream(&format!("if $x :\n{body}tail ( 1 ) ;\n")),
            corpus.stream(&format!("while $y :\n{body}other ( 2 ) ;\n")),
        ];
        let outcome = run(&streams, budget(4, 1));
        // `if`/`while` differ, so the shared run starts at `$x`. Line
        // alignment drops the header and both markers, which are not source
        // tokens and so can neither start nor end a block.
        assert_eq!(located(&streams, &outcome), [[(0, 2, 4), (1, 2, 4)]]);
        let block = &outcome.blocks[0];
        let tokens = &streams[0].tokens[block.spans[0].start..block.spans[0].end];
        assert!(tokens.iter().all(|token| !token.marker));
    }

    #[test]
    fn a_block_that_closes_a_layout_block_keeps_its_end_marker() {
        let mut corpus = Corpus::new();
        let body = "for $r in $rows :\nBS use ( $r ) ;\nkeep ( $r ) ; BE\n";
        let streams = vec![
            corpus.stream(&format!("head_a ;\n{body}tail_a ;\n")),
            corpus.stream(&format!("head_b ;\n{body}tail_b ;\n")),
        ];
        let outcome = run(&streams, budget(4, 1));
        assert_eq!(located(&streams, &outcome), [[(0, 2, 4), (1, 2, 4)]]);
        let span = outcome.blocks[0].spans[0];
        let tokens = &streams[0].tokens[span.start..span.end];
        assert_eq!(tokens.iter().filter(|token| token.marker).count(), 2);
        assert!(tokens.last().unwrap().marker);
    }

    #[test]
    fn nested_and_overlapping_blocks_are_reported_only_when_they_add_a_location() {
        let mut corpus = Corpus::new();
        let core = "alpha ( 1 ) ;\nbeta ( 2 ) ;\n";
        let extra = "gamma ( 3 ) ;\ndelta ( 4 ) ;\n";
        let streams = vec![
            corpus.stream(&format!("one ( ) ;\n{core}{extra}end ;\n")),
            corpus.stream(&format!("two ( ) ;\n{core}{extra}stop ;\n")),
            corpus.stream(&format!("three ( ) ;\n{core}halt ;\n")),
        ];
        let outcome = run(&streams, budget(4, 1));
        // The long block in 0 and 1, then the short core because stream 2
        // adds a location. The core alone in 0 and 1 is never listed, nor
        // is any shorter piece of the long block.
        assert_eq!(
            located(&streams, &outcome),
            [
                vec![(0, 2, 5), (1, 2, 5)],
                vec![(0, 2, 3), (1, 2, 3), (2, 2, 3)]
            ]
        );
    }

    #[test]
    fn a_periodic_run_is_reported_once_not_at_every_shift() {
        let mut corpus = Corpus::new();
        let streams = vec![
            corpus.stream(&format!("head_a ;\n{}end ;\n", "row ( 1 ) ,\n".repeat(6))),
            corpus.stream(&format!("head_b ;\n{}end ;\n", "row ( 1 ) ,\n".repeat(7))),
        ];
        let outcome = run(&streams, budget(4, 2));
        // The longest alignment pairs the six rows and `end` of the first
        // function with the last six rows and `end` of the second. Rows 1-6
        // of both, the same table one row shifted, overlap it in both
        // functions and add no location.
        assert_eq!(located(&streams, &outcome), [[(0, 2, 8), (1, 3, 9)]]);
    }

    #[test]
    fn repetition_inside_one_function_is_not_reported() {
        let mut corpus = Corpus::new();
        let streams = vec![
            corpus.stream(&format!("{SHARED}middle ;\n{SHARED}")),
            corpus.stream("unrelated ( 1 ) ;\nother ( 2 ) ;\n"),
        ];
        assert!(run(&streams, budget(4, 1)).blocks.is_empty());
    }

    #[test]
    fn streams_of_different_languages_never_match() {
        let mut corpus = Corpus::new();
        let streams = vec![
            corpus.stream_in(0, &format!("a ;\n{SHARED}")),
            corpus.stream_in(1, &format!("b ;\n{SHARED}")),
        ];
        assert!(run(&streams, budget(4, 1)).blocks.is_empty());
    }

    #[test]
    fn blocks_inside_members_of_one_whole_function_group_are_excluded() {
        let mut corpus = Corpus::new();
        let streams = vec![
            corpus.stream(&format!("a ;\n{SHARED}")),
            corpus.stream(&format!("b ;\n{SHARED}")),
            corpus.stream(&format!("c ;\n{SHARED}")),
        ];
        // Streams 0 and 1 are one group: a block only in them is excluded;
        // the block also in stream 2 is not.
        let in_group = |spans: &[Span]| spans.iter().all(|span| span.stream < 2);
        let all = find_blocks(&streams, budget(4, 1), false, in_group, u64::MAX);
        assert_eq!(all.blocks.len(), 1);
        assert_eq!(all.blocks[0].spans.len(), 3);

        let two = &streams[..2];
        let none = find_blocks(two, budget(4, 1), false, in_group, u64::MAX);
        assert!(none.blocks.is_empty());
    }

    #[test]
    fn exhausted_window_budget_stops_before_the_next_function_and_says_so() {
        let mut corpus = Corpus::new();
        let streams = vec![
            corpus.stream(&format!("a ;\n{SHARED}")),
            corpus.stream(&format!("b ;\n{SHARED}")),
            corpus.stream(&format!("c ;\n{SHARED}")),
        ];
        let full = run(&streams, budget(4, 1));
        let per_stream = full.windows_used / 3;
        let mut limits = budget(4, 1);
        limits.max_windows = per_stream * 2;
        let cut = run(&streams, limits);
        assert_eq!(cut.exhausted, Some(BlockBudgetKind::MaxWindows));
        assert_eq!(cut.functions_searched, 2);
        assert_eq!(cut.windows_used, per_stream * 2);
        assert_eq!(located(&streams, &cut), [[(0, 2, 5), (1, 2, 5)]]);
    }

    #[test]
    fn exhausted_comparison_and_block_budgets_are_reported() {
        let mut corpus = Corpus::new();
        let streams = vec![
            corpus.stream(&format!("a ;\n{SHARED}x ;\nfirst ( 1 ) ;\nfirst ( 2 ) ;\n")),
            corpus.stream(&format!("b ;\n{SHARED}y ;\nfirst ( 1 ) ;\nfirst ( 2 ) ;\n")),
        ];
        let full = run(&streams, budget(4, 1));
        assert_eq!(full.blocks.len(), 2);
        assert_eq!(full.exhausted, None);

        let mut limits = budget(4, 1);
        limits.max_comparisons = 1;
        let starved = run(&streams, limits);
        assert_eq!(starved.exhausted, Some(BlockBudgetKind::MaxComparisons));
        assert!(starved.blocks.is_empty());
        assert!(starved.comparisons_used <= 1);

        limits = budget(4, 1);
        limits.max_blocks = 1;
        let capped = run(&streams, limits);
        assert_eq!(capped.exhausted, Some(BlockBudgetKind::MaxBlocks));
        assert_eq!(capped.blocks, full.blocks[..1]);
    }

    #[test]
    fn hash_collisions_are_verified_by_comparing_tokens() {
        let mut corpus = Corpus::new();
        let streams = vec![
            corpus.stream(&format!("a ;\n{SHARED}x ;\nfirst ( 1 ) ;\nfirst ( 2 ) ;\n")),
            corpus.stream(&format!("b ;\n{SHARED}y ;\nfirst ( 1 ) ;\nfirst ( 2 ) ;\n")),
            corpus.stream("q ( 1 ) ;\nr ( 2 ) ;\ns ( 3 ) ;\n"),
        ];
        let honest = run(&streams, budget(4, 1));
        // Mask 0: every window lands in one hash bucket.
        let colliding = find_blocks(&streams, budget(4, 1), false, |_| false, 0);
        assert_eq!(honest.blocks, colliding.blocks);
        assert_eq!(honest.blocks.len(), 2);
    }

    #[test]
    fn the_search_is_deterministic() {
        let mut corpus = Corpus::new();
        let streams = vec![
            corpus.stream(&format!("a ;\n{SHARED}x ;\nfirst ( 1 ) ;\nfirst ( 2 ) ;\n")),
            corpus.stream(&format!("b ;\n{SHARED}y ;\nfirst ( 1 ) ;\nfirst ( 2 ) ;\n")),
            corpus.stream(&format!("c ;\n{SHARED}")),
        ];
        let one = run(&streams, budget(4, 1));
        let two = run(&streams, budget(4, 1));
        assert_eq!(one.blocks, two.blocks);
        assert_eq!(one.comparisons_used, two.comparisons_used);
        assert_eq!(one.windows_used, two.windows_used);
    }

    #[test]
    fn consistent_policy_splits_occurrences_by_identifier_pattern() {
        let mut corpus = Corpus::new();
        let repeated = "let $a = $b + $b ;\nuse ( $a , $b ) ;\nkeep ( $a ) ;\n";
        let renamed = "let $c = $d + $d ;\nuse ( $c , $d ) ;\nkeep ( $c ) ;\n";
        let distinct = "let $c = $d + $e ;\nuse ( $c , $d ) ;\nkeep ( $c ) ;\n";
        let streams = vec![
            corpus.stream(&format!("one ;\n{repeated}")),
            corpus.stream(&format!("two ;\n{renamed}")),
            corpus.stream(&format!("three ;\n{distinct}")),
        ];
        let broad = run(&streams, budget(4, 1));
        assert_eq!(broad.blocks.len(), 1);
        assert_eq!(broad.blocks[0].spans.len(), 3);
        let consistent = find_blocks(&streams, budget(4, 1), true, |_| false, u64::MAX);
        assert_eq!(
            located(&streams, &consistent),
            [[(0, 2, 4), (1, 2, 4)]],
            "the non-one-to-one occurrence is split off"
        );
    }

    #[test]
    fn line_flags_follow_source_tokens_and_skip_markers() {
        let mut corpus = Corpus::new();
        let stream = corpus.stream("a b\nBS c\nd BE\n");
        let flags: Vec<(bool, bool)> = stream
            .tokens
            .iter()
            .map(|t| (t.line_first, t.line_last))
            .collect();
        assert_eq!(
            flags,
            [
                (true, false),
                (false, true),
                (false, false),
                (true, true),
                (true, true),
                (false, false)
            ]
        );
    }
}
