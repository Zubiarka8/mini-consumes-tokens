//! Estimated token budgets and progressive detail for retrieval responses.
//!
//! Pure selection logic: ranking, de-duplication, pagination and fitting a
//! page into a budget. It knows nothing about TOON or MCP; the caller supplies
//! a `cost` closure that renders the *whole* response (headers, field names,
//! paths, line numbers, delimiters, truncation and pagination lines) and
//! counts it, so the budget covers what is actually sent.
//!
//! Token counts are **estimates** unless a [`TokenCounter`] says otherwise
//! through its [`Basis`]. [`EstimatedCounter`] is a heuristic with no measured
//! accuracy: `max_tokens` is an estimated budget, never a real model limit.
//!
//! Pipeline, in this order: rank → de-duplicate → paginate (`offset`,
//! `max_results`) → fit to the budget. [`Selection::next_offset`] counts
//! ranked, de-duplicated candidates consumed, so on a stable index consecutive
//! pages neither skip nor repeat a candidate.

use std::collections::HashMap;
use std::str::FromStr;

/// Upper bound for `max_tokens`; larger requests are clamped.
pub const MAX_BUDGET_TOKENS: usize = 1_000_000;
/// Upper bound for `max_results`; larger requests are clamped.
pub const MAX_BUDGET_RESULTS: usize = 1_000;

/// Invalid budget arguments.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BudgetError {
    #[error("max_tokens must be at least 1")]
    ZeroMaxTokens,
    #[error("max_results must be at least 1")]
    ZeroMaxResults,
    #[error("unknown detail level `{0}`: expected overview, summary or detailed")]
    UnknownDetail(String),
}

/// How much of each result is returned. Ordered from least to most.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DetailLevel {
    /// Identity and location only.
    Overview,
    /// Overview plus signature, a relevant fragment and relation counts.
    Summary,
    /// The complete, pre-budget response.
    #[default]
    Detailed,
}

impl DetailLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Summary => "summary",
            Self::Detailed => "detailed",
        }
    }

    /// The next less detailed level, `None` at [`DetailLevel::Overview`].
    pub fn lower(self) -> Option<Self> {
        match self {
            Self::Overview => None,
            Self::Summary => Some(Self::Overview),
            Self::Detailed => Some(Self::Summary),
        }
    }

    /// The next more detailed level, `None` at [`DetailLevel::Detailed`].
    pub fn higher(self) -> Option<Self> {
        match self {
            Self::Overview => Some(Self::Summary),
            Self::Summary => Some(Self::Detailed),
            Self::Detailed => None,
        }
    }
}

impl FromStr for DetailLevel {
    type Err = BudgetError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "overview" => Ok(Self::Overview),
            "summary" => Ok(Self::Summary),
            "detailed" => Ok(Self::Detailed),
            _ => Err(BudgetError::UnknownDetail(raw.to_string())),
        }
    }
}

/// Validated retrieval budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextBudget {
    max_tokens: Option<usize>,
    max_results: usize,
    detail: DetailLevel,
}

impl ContextBudget {
    /// `max_tokens: None` means no token budget (the pre-budget behavior).
    /// Zero values are rejected; oversized ones are clamped.
    pub fn new(
        max_tokens: Option<usize>,
        max_results: usize,
        detail: DetailLevel,
    ) -> Result<Self, BudgetError> {
        if max_tokens == Some(0) {
            return Err(BudgetError::ZeroMaxTokens);
        }
        if max_results == 0 {
            return Err(BudgetError::ZeroMaxResults);
        }
        Ok(Self {
            max_tokens: max_tokens.map(|n| n.min(MAX_BUDGET_TOKENS)),
            max_results: max_results.min(MAX_BUDGET_RESULTS),
            detail,
        })
    }

    pub fn max_tokens(&self) -> Option<usize> {
        self.max_tokens
    }

    pub fn max_results(&self) -> usize {
        self.max_results
    }

    pub fn detail(&self) -> DetailLevel {
        self.detail
    }
}

/// How a token count was obtained. Never conflate the three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// A heuristic; may differ from any real tokenizer.
    Estimated,
    /// Counted by the named tokenizer (not necessarily the serving model's).
    Measured(&'static str),
    /// Reported by the provider. MCT never produces this itself.
    Billed,
}

impl Basis {
    pub fn is_exact(self) -> bool {
        !matches!(self, Self::Estimated)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenCount {
    pub value: usize,
    pub basis: Basis,
}

pub trait TokenCounter {
    fn count(&self, text: &str) -> TokenCount;
}

/// Dependency-free heuristic: ASCII word runs cost one token per four
/// characters (rounded up), each ASCII punctuation mark one, each non-ASCII
/// character one, whitespace nothing. Its accuracy against real tokenizers is
/// not guaranteed, so it always reports [`Basis::Estimated`].
#[derive(Debug, Clone, Copy, Default)]
pub struct EstimatedCounter;

impl TokenCounter for EstimatedCounter {
    fn count(&self, text: &str) -> TokenCount {
        let mut tokens = 0usize;
        let mut run = 0usize;
        for ch in text.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                run += 1;
                continue;
            }
            tokens = tokens.saturating_add(run.div_ceil(4));
            run = 0;
            if !ch.is_whitespace() {
                tokens = tokens.saturating_add(1);
            }
        }
        tokens = tokens.saturating_add(run.div_ceil(4));
        TokenCount {
            value: tokens,
            basis: Basis::Estimated,
        }
    }
}

/// What a candidate is evidence of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Evidence {
    Definition,
    Fragment,
    Relation,
}

/// One retrievable result with its provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub symbol_id: i64,
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub evidence: Evidence,
    /// Relation role (`caller`, `callee`, …); empty for other evidence.
    pub role: String,
    /// Higher is more relevant. NaN ranks last.
    pub score: f64,
}

/// A candidate chosen for the response and the level it is rendered at.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedRow {
    pub candidate: Candidate,
    pub level: DetailLevel,
    pub requested: DetailLevel,
}

impl SelectedRow {
    pub fn degraded(&self) -> bool {
        self.level < self.requested
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub rows: Vec<SelectedRow>,
    /// Ranked, de-duplicated candidates in total.
    pub total: usize,
    pub offset: usize,
    /// Where the next page starts; `None` when everything has been returned.
    /// Equal to `offset` when the budget fits no row (see `needed_min_tokens`).
    pub next_offset: Option<usize>,
    /// Rows lowered below the requested level to fit the budget.
    pub degraded: usize,
    /// Page rows removed to fit the budget.
    pub dropped: usize,
    /// Estimated cost of the final response, as reported by `cost`.
    pub spent: usize,
    /// Even the minimal response exceeds `max_tokens`.
    pub over_budget: bool,
    /// When no row fits: the estimated cost of a response with one
    /// [`DetailLevel::Overview`] row.
    pub needed_min_tokens: Option<usize>,
}

impl Selection {
    pub fn truncated(&self) -> bool {
        self.next_offset.is_some()
    }

    /// The budget fit no row although candidates remained.
    pub fn minimal(&self) -> bool {
        self.rows.is_empty() && self.next_offset.is_some_and(|next| next == self.offset)
    }
}

fn quantize(score: f64) -> i64 {
    if score.is_nan() {
        i64::MIN
    } else {
        (score * 1_000_000.0).round() as i64
    }
}

/// Rank by quantized score (descending) with a total tie-break, then drop
/// exact duplicates and fragments contained in a better-ranked fragment of the
/// same symbol. Overlapping spans of different symbols, files or roles are
/// distinct evidence and are kept.
fn rank_and_dedup(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    candidates.sort_by(|a, b| {
        quantize(b.score)
            .cmp(&quantize(a.score))
            .then(a.evidence.cmp(&b.evidence))
            .then_with(|| a.path.cmp(&b.path))
            .then(a.start_line.cmp(&b.start_line))
            .then(b.end_line.cmp(&a.end_line))
            .then(a.symbol_id.cmp(&b.symbol_id))
            .then_with(|| a.role.cmp(&b.role))
    });
    let mut kept: Vec<Candidate> = Vec::with_capacity(candidates.len());
    let mut by_symbol: HashMap<(i64, Evidence), Vec<usize>> = HashMap::new();
    for candidate in candidates {
        let slot = by_symbol
            .entry((candidate.symbol_id, candidate.evidence))
            .or_default();
        let redundant = slot.iter().any(|&index| {
            let other = &kept[index];
            other.path == candidate.path
                && other.role == candidate.role
                && if candidate.evidence == Evidence::Fragment {
                    other.start_line <= candidate.start_line && candidate.end_line <= other.end_line
                } else {
                    other.start_line == candidate.start_line && other.end_line == candidate.end_line
                }
        });
        if !redundant {
            slot.push(kept.len());
            kept.push(candidate);
        }
    }
    kept
}

/// Select one page of `candidates` and fit it to `budget`.
///
/// `cost(rows, truncated)` must render the whole response for `rows` — with
/// its truncation footer when `truncated` — and return its estimated token
/// count. Without `max_tokens` it is never called.
///
/// Over budget, the least relevant rows are lowered one level at a time
/// (every degradation is reported through [`SelectedRow::degraded`] and
/// [`Selection::degraded`]), and only once every row is at
/// [`DetailLevel::Overview`] are rows dropped from the tail. Each step strictly
/// reduces the number of rows or the sum of levels, so the loop is bounded.
/// When not even one row fits, the response has no rows, [`Selection::minimal`]
/// is true and `needed_min_tokens` says what to raise `max_tokens` to; if
/// even that exceeds the budget `over_budget` is set. The caller must still
/// render this minimal, valid response.
pub fn select<F>(
    candidates: Vec<Candidate>,
    offset: usize,
    budget: &ContextBudget,
    mut cost: F,
) -> Selection
where
    F: FnMut(&[SelectedRow], bool) -> usize,
{
    let ranked = rank_and_dedup(candidates);
    let total = ranked.len();
    let requested = budget.detail();
    let mut rows: Vec<SelectedRow> = ranked
        .into_iter()
        .skip(offset)
        .take(budget.max_results())
        .map(|candidate| SelectedRow {
            candidate,
            level: requested,
            requested,
        })
        .collect();
    let page_len = rows.len();
    let first = rows.first().cloned();
    let truncated_after = |len: usize| offset.saturating_add(len) < total;

    let mut spent = 0;
    let mut over_budget = false;
    if let Some(limit) = budget.max_tokens() {
        spent = cost(&rows, truncated_after(rows.len()));
        while spent > limit {
            if let Some(row) = rows
                .iter_mut()
                .rev()
                .find(|row| row.level.lower().is_some())
            {
                if let Some(lower) = row.level.lower() {
                    row.level = lower;
                }
            } else if rows.pop().is_none() {
                over_budget = true;
                break;
            }
            spent = cost(&rows, truncated_after(rows.len()));
        }
    }

    let needed_min_tokens = match (&first, rows.is_empty(), budget.max_tokens()) {
        (Some(first), true, Some(_)) => Some(cost(
            &[SelectedRow {
                candidate: first.candidate.clone(),
                level: DetailLevel::Overview,
                requested,
            }],
            truncated_after(1),
        )),
        _ => None,
    };
    let degraded = rows.iter().filter(|row| row.degraded()).count();
    let next = offset.saturating_add(rows.len());
    Selection {
        dropped: page_len - rows.len(),
        degraded,
        total,
        offset,
        next_offset: (next < total).then_some(next),
        spent,
        over_budget,
        needed_min_tokens,
        rows,
    }
}
