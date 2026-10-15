//! On-demand report of structurally duplicated functions and methods
//! (issue #207, Phase 0): Type-2 clones whose token sequences are identical
//! after identifiers and literals are normalized.
//!
//! Read-only: never runs during `reindex`/`reindex_paths`, never persists
//! groups or fingerprints, never mutates the index or files. Tokens come from
//! the caller's [`TokenSourceRegistry`], because this crate knows no grammar.
//! Every group is a candidate for human review, not proof of redundancy or
//! equivalent behavior.

mod analysis;

use mct_core::TokenSourceRegistry;

use crate::{Index, Result};

/// Default minimum token-bearing lines a candidate needs.
pub const DEFAULT_MIN_LINES: usize = 6;
/// Default minimum tokens (counted before normalization) a candidate needs.
pub const DEFAULT_MIN_TOKENS: usize = 40;
/// Symbol kinds compared: functions and methods only.
pub const DUPLICATE_KIND_ALLOWLIST: &[&str] = &["function", "method"];
/// Most source lines kept in a member's [`DuplicateMember::excerpt`].
pub const MAX_EXCERPT_LINES: usize = 3;
/// Most characters kept per excerpt line.
pub const MAX_EXCERPT_LINE_CHARS: usize = 120;

/// Thresholds a candidate must meet, both of them, to be grouped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateOptions {
    /// Minimum lines containing at least one token, comments and blank lines
    /// excluded.
    pub min_lines: usize,
    /// Minimum tokens, counted before identifiers and literals are replaced.
    pub min_tokens: usize,
}

impl Default for DuplicateOptions {
    fn default() -> Self {
        Self {
            min_lines: DEFAULT_MIN_LINES,
            min_tokens: DEFAULT_MIN_TOKENS,
        }
    }
}

/// How generated code is treated. Phase 0 adds no marker or file-name
/// heuristic: generated files are left out only when the project's existing
/// exclusion rules (built-in exclusions, `.mctignore`, `@import-gitignore`)
/// already keep them out of the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratedCodeHandling {
    ConfiguredExclusionsOnly,
}

impl GeneratedCodeHandling {
    /// One-line explanation for reports.
    pub fn description(self) -> &'static str {
        match self {
            Self::ConfiguredExclusionsOnly => {
                "generated code is excluded only by the project's configured exclusion rules; \
                 no generated-code marker is detected"
            }
        }
    }
}

/// The analysis result. Deterministic for the same tree, index and options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateReport {
    /// Groups ordered by `line_count × members.len()` descending, ties broken
    /// by the first member's path and start line.
    pub groups: Vec<DuplicateGroup>,
    /// Candidates that could not be analyzed, ordered by path and start line.
    pub skipped: Vec<SkippedCandidate>,
    /// `false` when any candidate was skipped.
    pub complete: bool,
    pub options: DuplicateOptions,
    /// Candidates whose tokens were extracted, whether or not they met the
    /// thresholds.
    pub candidates_analyzed: usize,
    /// Of `candidates_analyzed`, those below `min_lines` or `min_tokens`.
    pub below_threshold: usize,
    /// Languages with a token source, sorted.
    pub supported_languages: Vec<String>,
    /// Indexed languages that have function/method symbols but no token
    /// source, sorted. Their symbols are not candidates and not skipped.
    pub unsupported_languages: Vec<String>,
    pub generated_code: GeneratedCodeHandling,
}

/// Two or more same-language members sharing one normalized token sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    /// Ordered by path, then start line.
    pub members: Vec<DuplicateMember>,
    /// Language shared by every member.
    pub language: String,
    /// Smallest token-bearing line count among the members.
    pub line_count: usize,
    /// Token count, the same for every member.
    pub token_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateMember {
    /// Project-relative path, forward slashes.
    pub path: String,
    /// 1-based, inclusive.
    pub start_line: u32,
    /// 1-based, inclusive.
    pub end_line: u32,
    pub symbol_name: String,
    /// Indexed kind, `function` or `method`.
    pub symbol_kind: String,
    pub language: String,
    /// Token-bearing lines of this member.
    pub line_count: usize,
    pub token_count: usize,
    /// Up to [`MAX_EXCERPT_LINES`] leading source lines of the member, each
    /// cut to [`MAX_EXCERPT_LINE_CHARS`] characters.
    pub excerpt: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkipReason {
    /// The current file could not be read within the repository limits.
    ReadError,
    /// The token source failed, or the candidate overlaps a syntax error.
    ParseError,
    /// The file changed since it was indexed.
    StaleLocation,
    /// The indexed range is empty, inverted, missing its end or outside the
    /// file.
    InvalidRange,
}

impl SkipReason {
    /// Stable kebab-case name for reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadError => "read-error",
            Self::ParseError => "parse-error",
            Self::StaleLocation => "stale-location",
            Self::InvalidRange => "invalid-range",
        }
    }
}

/// A candidate left out of the analysis, with enough context to diagnose it
/// but never the file's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedCandidate {
    pub path: String,
    pub start_line: u32,
    pub end_line: Option<u32>,
    pub symbol_name: String,
    pub language: String,
    pub reason: SkipReason,
    /// Short diagnostic, e.g. the I/O error or the syntax-error line.
    pub detail: String,
}

/// Groups indexed functions and methods of every language in `token_sources`
/// by identical normalized token sequence, reading each file's current
/// content. Fails only when the index cannot be queried; a per-candidate
/// failure becomes a [`SkippedCandidate`].
pub fn find_duplicate_groups(
    index: &Index,
    token_sources: &TokenSourceRegistry,
    options: DuplicateOptions,
) -> Result<DuplicateReport> {
    analysis::run(index, token_sources, options)
}
