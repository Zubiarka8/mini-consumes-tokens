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

/// Comparison policy, candidate thresholds, filters and report controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateOptions {
    /// Minimum lines containing at least one token, comments and blank lines
    /// excluded.
    pub min_lines: usize,
    /// Minimum tokens, counted before identifiers and literals are replaced.
    pub min_tokens: usize,
    pub normalization: Normalization,
    pub ranking: DuplicateRanking,
    /// Exact file or directory prefix, with repository-relative `/` paths.
    pub path: Option<String>,
    pub language: Option<String>,
    pub limit: Option<usize>,
    pub offset: usize,
    pub excerpts: bool,
    /// Where groups whose members only make flat calls are ranked.
    pub trivial_calls: TrivialCallPolicy,
    /// Repeated partial-block search and its budgets; `None` (the default)
    /// skips it and leaves the report exactly as without the feature.
    pub partial_blocks: Option<PartialBlockBudget>,
}

/// Default minimum source tokens of a repeated partial block.
pub const DEFAULT_BLOCK_MIN_TOKENS: usize = 40;
/// Default minimum token-bearing lines of a repeated partial block, in every
/// occurrence.
pub const DEFAULT_BLOCK_MIN_LINES: usize = 6;
/// Default most token windows hashed. Each costs 16 bytes of search memory,
/// so the default bounds the window table to about 32 MB.
pub const DEFAULT_BLOCK_MAX_WINDOWS: usize = 2_000_000;
/// Default most token comparisons spent extending candidate blocks; bounds
/// the search's running time deterministically instead of by wall clock.
pub const DEFAULT_BLOCK_MAX_COMPARISONS: usize = 20_000_000;
/// Default most partial blocks reported.
pub const DEFAULT_BLOCK_MAX_BLOCKS: usize = 200;
/// What the partial-block search leaves unevaluated; it replaces
/// `partial-block repetition` in the report's not-evaluated list when the
/// search runs.
pub const PARTIAL_BLOCKS_NOT_EVALUATED: &str =
    "extractability of repeated partial blocks (variables in/out, control flow, side effects)";

/// Thresholds and budgets of the repeated partial-block search. Every budget
/// is a deterministic count, so the same tree and options always stop at the
/// same point; an exhausted budget is reported, never silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialBlockBudget {
    /// Minimum source tokens per block (block markers excluded).
    pub min_tokens: usize,
    /// Minimum token-bearing lines per block, in every occurrence.
    pub min_lines: usize,
    /// Most token windows hashed (memory bound).
    pub max_windows: usize,
    /// Most token comparisons spent extending candidate blocks (time bound).
    pub max_comparisons: usize,
    /// Most blocks reported, in report order.
    pub max_blocks: usize,
}

impl Default for PartialBlockBudget {
    fn default() -> Self {
        Self {
            min_tokens: DEFAULT_BLOCK_MIN_TOKENS,
            min_lines: DEFAULT_BLOCK_MIN_LINES,
            max_windows: DEFAULT_BLOCK_MAX_WINDOWS,
            max_comparisons: DEFAULT_BLOCK_MAX_COMPARISONS,
            max_blocks: DEFAULT_BLOCK_MAX_BLOCKS,
        }
    }
}

/// The budget that stopped the partial-block search first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockBudgetKind {
    /// Functions after the cut-off were not searched at all.
    MaxWindows,
    /// Candidate blocks after the cut-off were not extended.
    MaxComparisons,
    /// More blocks were found than are listed.
    MaxBlocks,
}

impl BlockBudgetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MaxWindows => "max-windows",
            Self::MaxComparisons => "max-comparisons",
            Self::MaxBlocks => "max-blocks",
        }
    }
}

/// Result of the repeated partial-block search.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PartialBlockReport {
    /// Functions whose token streams were searched (outermost callables that
    /// meet the block thresholds, before any budget cut-off).
    pub functions_searched: usize,
    pub windows_used: usize,
    pub comparisons_used: usize,
    /// `None` when every budget sufficed.
    pub budget_exhausted: Option<BlockBudgetKind>,
    /// Ordered by tokens, then occurrence count (both descending), then the
    /// first occurrence's path and start line; `id` follows this order.
    pub blocks: Vec<PartialBlock>,
}

/// One repeated token sequence found in two or more different functions.
/// It is evidence of repetition only: whether it could be extracted (values
/// flowing in and out, control flow, side effects) is never evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialBlock {
    /// 1-based position in the report.
    pub id: usize,
    pub language: String,
    /// Source tokens, the same in every occurrence.
    pub tokens: usize,
    /// Smallest token-bearing line count among the occurrences.
    pub lines: usize,
    /// One per function, ordered by path and start line.
    pub occurrences: Vec<BlockOccurrence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockOccurrence {
    pub path: String,
    /// 1-based, inclusive.
    pub start_line: u32,
    /// 1-based, inclusive.
    pub end_line: u32,
    /// Innermost analyzed function or method containing the occurrence.
    pub function: String,
}

impl Default for DuplicateOptions {
    fn default() -> Self {
        Self {
            min_lines: DEFAULT_MIN_LINES,
            min_tokens: DEFAULT_MIN_TOKENS,
            normalization: Normalization::Broad,
            ranking: DuplicateRanking::Lines,
            path: None,
            language: None,
            limit: None,
            offset: 0,
            excerpts: true,
            trivial_calls: TrivialCallPolicy::Demote,
            partial_blocks: None,
        }
    }
}

/// Treatment of groups whose every member's body is only flat calls (see
/// [`mct_core::BodyShape`]). No policy removes a group, and none looks at a
/// callee's name: `log`, `print` or `assert` may be redefined or matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrivialCallPolicy {
    /// Rank such groups after every other group, keeping their relative order.
    Demote,
    /// Flag them but keep the ranking order.
    Keep,
}

impl TrivialCallPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Demote => "demote",
            Self::Keep => "keep",
        }
    }
}

impl std::str::FromStr for TrivialCallPolicy {
    type Err = &'static str;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "demote" => Ok(Self::Demote),
            "keep" => Ok(Self::Keep),
            _ => Err("expected demote or keep"),
        }
    }
}

/// All modes ignore trivia. Consistency compares spellings, not resolved bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Normalization {
    Broad,
    Consistent,
    Exact,
}

impl Normalization {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Broad => "broad",
            Self::Consistent => "consistent",
            Self::Exact => "exact",
        }
    }
}

impl std::str::FromStr for Normalization {
    type Err = &'static str;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "broad" => Ok(Self::Broad),
            "consistent" => Ok(Self::Consistent),
            "exact" => Ok(Self::Exact),
            _ => Err("expected broad, consistent or exact"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateRanking {
    Lines,
    Tokens,
}

impl DuplicateRanking {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lines => "lines",
            Self::Tokens => "tokens",
        }
    }
}

impl std::str::FromStr for DuplicateRanking {
    type Err = &'static str;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "lines" => Ok(Self::Lines),
            "tokens" => Ok(Self::Tokens),
            _ => Err("expected lines or tokens"),
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
    /// Full filtered group count, before output pagination.
    pub total_groups: usize,
    /// Of `total_groups`, those ranked last by [`TrivialCallPolicy::Demote`].
    pub demoted_groups: usize,
    /// Verdicts of the reported groups (`groups`), not of the whole total.
    pub verdict_counts: VerdictCounts,
    /// Of `total_groups`, those whose [`GroupSubgroups::status`] is split.
    pub split_groups: usize,
    /// Whole-index diagnostics, independent of candidate filters and output limits.
    pub index_syntax_errors: usize,
    pub index_read_failures: usize,
    pub has_indexing_record: bool,
    /// Selected page of groups in the requested ranking, ties broken by the
    /// first member's path and start line. Defaults to lines times members.
    /// Under [`TrivialCallPolicy::Demote`], demoted groups follow all others.
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
    /// Present only when [`DuplicateOptions::partial_blocks`] is set.
    pub partial_blocks: Option<PartialBlockReport>,
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
    /// Ranking priority and its reason. Computed for every group.
    pub priority: GroupPriority,
    /// Aligned comparison of the members' original tokens. Computed only for
    /// the reported page of groups.
    pub evidence: GroupEvidence,
    /// Explainable verdict derived from `evidence` and `priority`. Computed
    /// only for the reported page of groups, after `evidence`.
    pub verdict: GroupVerdict,
    /// Members partitioned by the evidence that separates them. An extra view:
    /// `members`, totals, order and pagination are unchanged. Computed for
    /// every group.
    pub subgroups: GroupSubgroups,
}

/// One explainable verdict per reported group, derived only from the group's
/// [`GroupEvidence`] and [`GroupPriority`] by a fixed rule table (ADR-010).
///
/// A heuristic explanation for review, not proof: `Real` does not prove that
/// members behave the same, and no current rule claims `FalsePositive`. A
/// verdict never removes, splits, reorders or filters a group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupVerdict {
    pub level: VerdictLevel,
    /// Every rule that fired, in [`VerdictReason`] order, without repeats.
    /// For `Suspicious`, the differences the detector cannot judge; for
    /// `Real`, what the evidence shows.
    pub reasons: Vec<VerdictReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerdictLevel {
    /// No difference beyond the declaration name and a consistent renaming of
    /// the members' own parameters (and docstrings) in the same context, and
    /// bodies that are not only flat calls.
    Real,
    /// At least one difference or unknown the detector cannot judge. Also the
    /// value of a group whose verdict was not computed.
    #[default]
    Suspicious,
    /// The evidence alone shows the members are not duplicates. Defined for
    /// the output contract; no current rule emits it (see ADR-010).
    FalsePositive,
}

impl VerdictLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Real => "REAL",
            Self::Suspicious => "SUSPICIOUS",
            Self::FalsePositive => "FALSE_POSITIVE",
        }
    }
}

/// Why a verdict was given. Codes that name a difference match the
/// corresponding [`WarningCode`] text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VerdictReason {
    // Supporting a `Real` verdict.
    /// Every aligned identifier has the same text.
    IdenticalTokens,
    /// Identifiers differ only by a one-to-one renaming.
    ConsistentRenaming,
    /// Decorators and enclosing constructs are the same text.
    SameContext,
    /// Docstrings differ; listed, but not a reason for suspicion.
    DocstringsDiffer,
    // Making a verdict `Suspicious`.
    AlignmentUnavailable,
    IdentifierPatternDiffers,
    /// A renamed `name` that is not a parameter of its own member: a renamed
    /// local, or a reference to a different outside item.
    UndeclaredNamesDiffer,
    MixedRoleDifferences,
    MemberNamesDiffer,
    CalleeNamesDiffer,
    TypeNamesDiffer,
    PrimitiveTypeSpellingsDiffer,
    LiteralCategoriesDiffer,
    LiteralValuesDiffer,
    InterpolatedStringsDiffer,
    ContextUnavailable,
    DecoratorsDiffer,
    EnclosingContextDiffers,
    CallOnlyBodies,
    CallShapeMixed,
    BodyShapeNotEvaluated,
}

impl VerdictReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IdenticalTokens => "identical-tokens",
            Self::ConsistentRenaming => "consistent-renaming",
            Self::SameContext => "same-context",
            Self::DocstringsDiffer => "docstrings-differ",
            Self::AlignmentUnavailable => "alignment-unavailable",
            Self::IdentifierPatternDiffers => "identifier-pattern-differs",
            Self::UndeclaredNamesDiffer => "undeclared-names-differ",
            Self::MixedRoleDifferences => "mixed-role-differences",
            Self::MemberNamesDiffer => "member-names-differ",
            Self::CalleeNamesDiffer => "callee-names-differ",
            Self::TypeNamesDiffer => "type-names-differ",
            Self::PrimitiveTypeSpellingsDiffer => "primitive-type-spellings-differ",
            Self::LiteralCategoriesDiffer => "literal-categories-differ",
            Self::LiteralValuesDiffer => "literal-values-differ",
            Self::InterpolatedStringsDiffer => "interpolated-strings-differ",
            Self::ContextUnavailable => "context-unavailable",
            Self::DecoratorsDiffer => "decorators-differ",
            Self::EnclosingContextDiffers => "enclosing-context-differs",
            Self::CallOnlyBodies => "call-only-bodies",
            Self::CallShapeMixed => "call-shape-mixed",
            Self::BodyShapeNotEvaluated => "body-shape-not-evaluated",
        }
    }
}

/// Verdicts per level over the reported groups (the selected page).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VerdictCounts {
    pub real: usize,
    pub suspicious: usize,
    pub false_positive: usize,
}

/// Most subgroup rows listed per group; [`GroupSubgroups::count`] covers all.
pub const MAX_LISTED_SUBGROUPS: usize = 8;
/// Most member indices listed per subgroup row; [`Subgroup::size`] covers all.
pub const MAX_LISTED_SUBGROUP_MEMBERS: usize = 10;
/// Most separating values shown per subgroup row.
pub const MAX_SUBGROUP_VALUES: usize = 3;

/// A deterministic partition of a group's members by equal evidence.
///
/// Two members share a subgroup exactly when they agree on every separating
/// evidence item: the text at each differing aligned `member`, `callee` and
/// `type` position, the literal category at each `literal-category` position,
/// and their identifier pattern (each spelling numbered by first use, so two
/// members have equal patterns exactly when one is a consistent renaming of
/// the other). A `member`, `callee` or `type` position is not separating when
/// every member's text there is also a name that member declares or takes as a
/// parameter (renaming, recursion). Individual names, parameters, literal
/// values, interpolated strings, docstrings, mixed roles, decorators and
/// enclosing constructs never separate. Nothing is resolved or scored.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupSubgroups {
    pub status: SubgroupStatus,
    /// Kinds of the separating items, in [`SubgroupBasis`] order; empty
    /// unless split.
    pub basis: Vec<SubgroupBasis>,
    /// Number of subgroups: 1 when single, 0 when not evaluated.
    pub count: usize,
    /// Separating aligned positions, plus one when identifier patterns differ.
    pub separating_items: usize,
    /// Every member is its own subgroup; no row is listed because the member
    /// list already shows the partition.
    pub all_distinct: bool,
    /// The first [`MAX_LISTED_SUBGROUPS`] subgroups, by first member; empty
    /// when single, not evaluated or all distinct.
    pub subgroups: Vec<Subgroup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubgroupStatus {
    /// Aligned tokens are unavailable for at least one member.
    #[default]
    NotEvaluated,
    /// No evidence separates the members.
    Single,
    /// At least two subgroups.
    Split,
}

impl SubgroupStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotEvaluated => "not-evaluated",
            Self::Single => "single",
            Self::Split => "split",
        }
    }
}

/// An evidence kind that can separate members.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SubgroupBasis {
    Member,
    Callee,
    Type,
    LiteralCategory,
    IdentifierPattern,
}

impl SubgroupBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Callee => "callee",
            Self::Type => "type",
            Self::LiteralCategory => "literal-category",
            Self::IdentifierPattern => "identifier-pattern",
        }
    }
}

/// Members that agree on every separating item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subgroup {
    /// 1-based, in order of each subgroup's first member.
    pub id: usize,
    pub size: usize,
    /// 0-based indices into [`DuplicateGroup::members`], ascending; at most
    /// [`MAX_LISTED_SUBGROUP_MEMBERS`].
    pub members: Vec<usize>,
    /// This subgroup's value at the first [`MAX_SUBGROUP_VALUES`] separating
    /// items, as `kind=text`, texts cut to [`MAX_DIFFERENCE_TEXT_CHARS`]; an
    /// identifier pattern shows as `identifier-pattern=#N`.
    pub values: Vec<String>,
}

/// Where a group is ranked and why. A demoted group is still reported in full.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupPriority {
    pub level: PriorityLevel,
    pub call_shape: CallShape,
    /// Always set once the report is built.
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PriorityLevel {
    #[default]
    Normal,
    Demoted,
}

impl PriorityLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Demoted => "demoted",
        }
    }
}

/// Whether the members' bodies are made only of flat calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CallShape {
    /// A member's token source reports no body shape.
    #[default]
    NotEvaluated,
    /// Some body statement is not a flat call, in every member.
    Other,
    /// Every member's body is only flat calls.
    CallOnly,
    /// Only some members' bodies are only flat calls.
    Mixed,
}

impl CallShape {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotEvaluated => "not-evaluated",
            Self::Other => "other",
            Self::CallOnly => "call-only",
            Self::Mixed => "mixed",
        }
    }
}

/// Most distinct aligned differences listed per group; counts cover all.
pub const MAX_LISTED_DIFFERENCES: usize = 12;
/// Most characters of one token's text shown in a difference.
pub const MAX_DIFFERENCE_TEXT_CHARS: usize = 60;
/// Criteria the aligned evidence does not evaluate, in report order.
pub const EVIDENCE_NOT_EVALUATED: &[&str] = &[
    "binding and scope resolution",
    "type compatibility",
    "effects of decorators and enclosing conditions",
    "effects of calls in call-only bodies",
    "partial-block repetition",
    "behavioral equivalence (verdicts are heuristic explanations and prove nothing)",
];

/// What the members' original token text shows once their normalized
/// sequences, equal by construction, are aligned position by position.
///
/// Evidence for review, not a verdict: no difference proves that members are
/// unrelated, and no absence of differences proves equivalent behavior.
/// Roles are the token sources' syntactic roles; nothing is resolved.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupEvidence {
    pub identifier_pattern: IdentifierPattern,
    /// Differing aligned positions per kind, in [`DifferenceKind`] order;
    /// kinds without a differing position are absent.
    pub differing_positions: Vec<(DifferenceKind, usize)>,
    /// Distinct differences: positions with the same kind and per-member
    /// texts count once.
    pub distinct_differences: usize,
    /// Differing `name` positions where at least one member's spelling is not
    /// one of that member's own parameters. Such a name
    /// may be a renamed local or a reference to a different outside item
    /// (a path, constant, unit struct or function); bindings are not resolved,
    /// so the two cannot be told apart.
    pub undeclared_name_positions: usize,
    /// The first [`MAX_LISTED_DIFFERENCES`] distinct differences, in the first
    /// member's source order.
    pub differences: Vec<AlignedDifference>,
    /// What the compared spans exclude: decorators and enclosing constructs.
    /// Kept apart from the aligned tokens; it never changes grouping.
    pub context: ContextEvidence,
    /// Primitive spelling evidence; actual compatibility always remains unknown.
    pub types: TypeEvidence,
    pub warnings: Vec<EvidenceWarning>,
}

/// Aligned type annotation evidence. No field establishes type identity,
/// compatibility or incompatibility: names can be shadowed in both languages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeEvidence {
    /// Positions with at least one syntactic type role, including equal texts.
    pub compared_positions: usize,
    /// Positions containing at least two distinct recognized primitive spellings.
    /// Mixed roles are excluded; other members may have unknown annotations.
    pub primitive_spelling_conflicts: usize,
    /// First conflict positions in source order, at most MAX_LISTED_DIFFERENCES.
    /// Values and lines follow member order; unrecognized values say `(unknown)`.
    pub conflicts: Vec<AlignedDifference>,
}

/// Comparison of the members' decorators (or outer attributes) and enclosing
/// constructs, as their token source reports them. Texts are compared, never
/// resolved: two spellings of one decorator differ, and no effect is inferred.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContextEvidence {
    pub status: ContextStatus,
    /// One row per aspect, in [`ContextAspect`] order; empty when not
    /// evaluated.
    pub aspects: Vec<ContextComparison>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextStatus {
    /// A member's token source reports no callable context.
    #[default]
    NotEvaluated,
    Same,
    Differs,
}

impl ContextStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotEvaluated => "not-evaluated",
            Self::Same => "same",
            Self::Differs => "differs",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextAspect {
    Decorators,
    Enclosing,
}

impl ContextAspect {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Decorators => "decorators",
            Self::Enclosing => "enclosing",
        }
    }
}

/// One aspect: the shared value when every member agrees, otherwise one value
/// per member in member order. Values are cut to [`MAX_DIFFERENCE_TEXT_CHARS`]
/// characters; `(none)` and `(top level)` stand for an empty aspect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextComparison {
    pub aspect: ContextAspect,
    pub differs: bool,
    pub values: Vec<String>,
}

/// Whether identifier spellings correspond one-to-one between the first
/// member and every other member.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum IdentifierPattern {
    /// Evidence was not computed (no aligned tokens available).
    #[default]
    NotEvaluated,
    /// Every aligned identifier has the same text.
    Identical,
    /// A consistent renaming: each spelling maps to exactly one spelling.
    OneToOne,
    /// One spelling maps to two; the detail names an example.
    NotOneToOne(String),
}

impl IdentifierPattern {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotEvaluated => "not-evaluated",
            Self::Identical => "identical",
            Self::OneToOne => "one-to-one",
            Self::NotOneToOne(_) => "not-one-to-one",
        }
    }
}

/// Kind of an aligned difference, from the token's role in the first member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DifferenceKind {
    Declaration,
    Parameter,
    /// An identifier with no syntactic role: locals, references, paths.
    Name,
    Member,
    Callee,
    Type,
    /// Literals of one category with different text.
    LiteralValue,
    /// Literals whose categories differ, such as number and string.
    LiteralCategory,
    InterpolatedString,
    Docstring,
    /// Members whose roles differ at this position.
    MixedRole,
}

impl DifferenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Declaration => "declaration",
            Self::Parameter => "parameter",
            Self::Name => "name",
            Self::Member => "member",
            Self::Callee => "callee",
            Self::Type => "type",
            Self::LiteralValue => "literal-value",
            Self::LiteralCategory => "literal-category",
            Self::InterpolatedString => "interpolated-string",
            Self::Docstring => "docstring",
            Self::MixedRole => "mixed-role",
        }
    }
}

/// One distinct difference: the text and first line of every member, in
/// member order. Texts are cut to [`MAX_DIFFERENCE_TEXT_CHARS`] characters;
/// a literal-category difference appends each category in parentheses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignedDifference {
    pub kind: DifferenceKind,
    pub occurrences: usize,
    pub values: Vec<String>,
    pub lines: Vec<u32>,
}

/// A difference worth a reviewer's attention, with its explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceWarning {
    pub code: WarningCode,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WarningCode {
    AlignmentUnavailable,
    IdentifierPatternDiffers,
    MemberNamesDiffer,
    CalleeNamesDiffer,
    TypeNamesDiffer,
    PrimitiveTypeSpellingsDiffer,
    LiteralCategoriesDiffer,
    LiteralValuesDiffer,
    InterpolatedStringsDiffer,
    ContextUnavailable,
    DecoratorsDiffer,
    EnclosingContextDiffers,
}

impl WarningCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AlignmentUnavailable => "alignment-unavailable",
            Self::IdentifierPatternDiffers => "identifier-pattern-differs",
            Self::MemberNamesDiffer => "member-names-differ",
            Self::CalleeNamesDiffer => "callee-names-differ",
            Self::TypeNamesDiffer => "type-names-differ",
            Self::PrimitiveTypeSpellingsDiffer => "primitive-type-spellings-differ",
            Self::LiteralCategoriesDiffer => "literal-categories-differ",
            Self::LiteralValuesDiffer => "literal-values-differ",
            Self::InterpolatedStringsDiffer => "interpolated-strings-differ",
            Self::ContextUnavailable => "context-unavailable",
            Self::DecoratorsDiffer => "decorators-differ",
            Self::EnclosingContextDiffers => "enclosing-context-differs",
        }
    }
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
    mut options: DuplicateOptions,
) -> Result<DuplicateReport> {
    if let Some(path) = &options.path {
        use std::path::{Component, Path};
        let mut parts = Vec::new();
        for component in Path::new(path).components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::CurDir => {}
                _ => return Err(crate::IndexError::PathEscapesRoot(path.clone())),
            }
        }
        options.path = (!parts.is_empty()).then(|| parts.join("/"));
    }
    analysis::run(index, token_sources, options)
}
