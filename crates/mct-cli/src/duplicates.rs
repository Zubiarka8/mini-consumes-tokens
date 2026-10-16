//! `mct-cli duplicates`: on-demand report of candidate structurally
//! duplicated functions and methods (issue #207, Phase 0).
//!
//! Read-only: opens an existing index, runs the analysis from `mct-index`
//! against the current files, and renders the [`DuplicateReport`] as TOON on
//! stdout. It never reindexes and never writes anything. The renderer is local
//! (not `mct_mcp_server::toon::encode_table`) because the report needs nested
//! blocks and a stricter quoting rule: source excerpts keep their leading
//! indentation and contain colons and brackets, which the shared table
//! encoder leaves unquoted.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context as _;
use mct_index::{
    find_duplicate_groups, ContextEvidence, DuplicateGroup, DuplicateMember, DuplicateOptions,
    DuplicateReport, ExcludeSet, GroupEvidence, Index, SkipReason, SkippedCandidate,
    SubgroupStatus,
};
use mct_languages::build_token_sources;

use crate::display_path;

/// First field of every report: groups are leads, not findings.
const NOTICE: &str = "Groups below are CANDIDATE duplicates for human review. \
Identical normalized structure does not prove that the functions are redundant \
or behave the same.";

#[derive(clap::Args, Debug)]
pub(crate) struct Args {
    /// Minimum source-token-bearing lines per candidate.
    #[arg(long, default_value_t = mct_index::DEFAULT_MIN_LINES, value_parser = parse_threshold)]
    min_lines: usize,
    /// Minimum source tokens per candidate.
    #[arg(long, default_value_t = mct_index::DEFAULT_MIN_TOKENS, value_parser = parse_threshold)]
    min_tokens: usize,
    /// Comparison policy: broad, consistent identifier spelling, or exact token text.
    #[arg(long, default_value = "broad")]
    normalization: mct_index::Normalization,
    /// Rank by lines times members, or tokens times additional copies (estimate).
    #[arg(long, default_value = "lines")]
    ranking: mct_index::DuplicateRanking,
    /// Exact file or directory prefix, relative to the repository root.
    #[arg(long)]
    path: Option<String>,
    /// Exact indexed language id.
    #[arg(long)]
    language: Option<String>,
    /// Maximum groups to display; totals describe all filtered groups.
    #[arg(long)]
    limit: Option<usize>,
    /// Skip this many ranked groups.
    #[arg(long, default_value_t = 0)]
    offset: usize,
    /// Omit source excerpts.
    #[arg(long)]
    no_excerpts: bool,
    /// Groups whose bodies are only flat calls: rank them last (demote) or
    /// keep the ranking order (keep). Never removes a group.
    #[arg(long, default_value = "demote")]
    trivial_calls: mct_index::TrivialCallPolicy,
}

impl From<Args> for DuplicateOptions {
    fn from(args: Args) -> Self {
        Self {
            min_lines: args.min_lines,
            min_tokens: args.min_tokens,
            normalization: args.normalization,
            ranking: args.ranking,
            path: args.path,
            language: args.language,
            limit: args.limit,
            offset: args.offset,
            excerpts: !args.no_excerpts,
            trivial_calls: args.trivial_calls,
        }
    }
}

/// Every skip reason, in the stable order the report lists them.
const SKIP_REASONS: [SkipReason; 4] = [
    SkipReason::ReadError,
    SkipReason::ParseError,
    SkipReason::StaleLocation,
    SkipReason::InvalidRange,
];

/// Clap value parser for `--min-lines` / `--min-tokens`: a positive integer.
pub(crate) fn parse_threshold(raw: &str) -> Result<usize, String> {
    match raw.trim().parse::<usize>() {
        Ok(0) => Err("must be at least 1".to_string()),
        Ok(value) => Ok(value),
        Err(_) => Err(format!("`{raw}` is not a positive integer")),
    }
}

/// Opens the project's existing index (never creating one), analyzes it and
/// prints the TOON report.
pub(crate) fn run(root: &Path, options: DuplicateOptions) -> anyhow::Result<()> {
    if !root.is_dir() {
        anyhow::bail!("project root {} is not a directory", display_path(root));
    }
    let db_path = root.join(".mct-index").join("index.sqlite3");
    if !db_path.is_file() {
        anyhow::bail!(
            "no index found at {}; run `mct-cli --root {} init` first",
            display_path(&db_path),
            display_path(root)
        );
    }
    let exclude = ExcludeSet::new(&mct_index::read_ignore_file(root));
    let index = Index::open_existing_read_only(root, &db_path, exclude)
        .with_context(|| format!("cannot open the index at {}", display_path(&db_path)))?;
    let report = find_duplicate_groups(&index, &build_token_sources(), options)
        .context("cannot query the index for duplicate candidates")?;
    print!("{}", render_toon(&report));
    Ok(())
}

/// Renders the report as TOON: scalar header fields, then a `groups` list of
/// objects (each with a `members` table) and a `skipped` table. Follows the
/// report's own order; nothing is re-sorted or hashed.
pub(crate) fn render_toon(report: &DuplicateReport) -> String {
    let mut out = String::new();
    field(&mut out, 0, "notice", NOTICE);
    field(
        &mut out,
        0,
        "status",
        if report.complete {
            "complete"
        } else {
            "partial"
        },
    );
    if !report.complete {
        field(
            &mut out,
            0,
            "partial_reason",
            "some candidates were skipped, so groups involving them may be missing; see skipped",
        );
    }
    out.push_str("options:\n");
    number(&mut out, 2, "min_lines", report.options.min_lines);
    number(&mut out, 2, "min_tokens", report.options.min_tokens);
    field(
        &mut out,
        2,
        "normalization",
        report.options.normalization.as_str(),
    );
    field(&mut out, 2, "ranking", report.options.ranking.as_str());
    if let Some(path) = &report.options.path {
        field(&mut out, 2, "path", path);
    }
    if let Some(language) = &report.options.language {
        field(&mut out, 2, "language", language);
    }
    number(&mut out, 2, "offset", report.options.offset);
    if let Some(limit) = report.options.limit {
        number(&mut out, 2, "limit", limit);
    }
    field(
        &mut out,
        2,
        "excerpts",
        if report.options.excerpts {
            "included"
        } else {
            "omitted"
        },
    );
    field(
        &mut out,
        2,
        "trivial_calls",
        report.options.trivial_calls.as_str(),
    );
    let coverage = if report.index_syntax_errors > 0 || report.index_read_failures > 0 {
        "partial"
    } else if report.has_indexing_record {
        "no-recorded-errors"
    } else {
        "unknown"
    };
    field(&mut out, 0, "index_coverage", coverage);
    field(
        &mut out,
        0,
        "coverage_scope",
        "whole index at last indexing; new unindexed files are not discovered",
    );
    number(
        &mut out,
        0,
        "index_syntax_errors",
        report.index_syntax_errors,
    );
    number(
        &mut out,
        0,
        "index_read_failures",
        report.index_read_failures,
    );
    inline_array(&mut out, "supported_languages", &report.supported_languages);
    inline_array(
        &mut out,
        "unsupported_languages",
        &report.unsupported_languages,
    );
    field(
        &mut out,
        0,
        "generated_code",
        report.generated_code.description(),
    );
    field(
        &mut out,
        0,
        "alignment_scope",
        "per group, the original identifier and literal tokens at aligned positions; \
         roles are syntactic and nothing is resolved; differences are evidence, not verdicts",
    );
    let not_evaluated: Vec<String> = mct_index::EVIDENCE_NOT_EVALUATED
        .iter()
        .map(|item| (*item).to_string())
        .collect();
    inline_array(&mut out, "alignment_not_evaluated", &not_evaluated);
    field(
        &mut out,
        0,
        "context_scope",
        "per group, each member's decorators or outer attributes and enclosing constructs \
         as text; compared, never resolved; a difference is a warning, not a verdict",
    );
    field(
        &mut out,
        0,
        "priority_scope",
        "per group, whether every member's body is only flat calls, read from syntax; \
         callee names are never considered; demote ranks such groups last, keep leaves \
         the ranking order; no group is removed",
    );
    field(
        &mut out,
        0,
        "verdict_scope",
        "per reported group, REAL, SUSPICIOUS or FALSE_POSITIVE from a fixed rule table over \
         the alignment, types, context and priority evidence, with every rule that fired as a \
         reason; heuristic explanations, not proof; REAL means no difference beyond a \
         consistent renaming in the same context; no current rule emits FALSE_POSITIVE; \
         verdicts never remove or reorder groups; counts cover the reported groups only",
    );
    field(
        &mut out,
        0,
        "subgroups_scope",
        "per group, members that agree on every differing member, callee and type text \
         (names a member declares or takes as a parameter excluded), literal category and \
         identifier pattern; decorators and enclosing constructs do not separate; an extra \
         view, membership, totals and pagination are unchanged; not a verdict",
    );
    number(
        &mut out,
        0,
        "candidates_analyzed",
        report.candidates_analyzed,
    );
    number(&mut out, 0, "below_threshold", report.below_threshold);
    number(&mut out, 0, "total_groups", report.total_groups);
    number(&mut out, 0, "demoted_groups", report.demoted_groups);
    out.push_str("verdict_counts:\n");
    number(&mut out, 2, "real", report.verdict_counts.real);
    number(&mut out, 2, "suspicious", report.verdict_counts.suspicious);
    number(
        &mut out,
        2,
        "false_positive",
        report.verdict_counts.false_positive,
    );
    number(&mut out, 0, "split_groups", report.split_groups);
    number(
        &mut out,
        0,
        "omitted_groups",
        report.total_groups.saturating_sub(report.groups.len()),
    );
    number(&mut out, 0, "total_skipped", report.skipped.len());
    out.push_str("skipped_by_reason:\n");
    for reason in SKIP_REASONS {
        let count = report
            .skipped
            .iter()
            .filter(|skipped| skipped.reason == reason)
            .count();
        number(&mut out, 2, reason.as_str(), count);
    }

    let _ = writeln!(out, "groups[{}]:", report.groups.len());
    for (position, group) in report.groups.iter().enumerate() {
        render_group(
            &mut out,
            report
                .options
                .offset
                .saturating_add(position)
                .saturating_add(1),
            group,
            report.options.normalization,
        );
    }

    let _ = writeln!(
        out,
        "skipped[{}]{{path,range,symbol,language,reason,detail}}:",
        report.skipped.len()
    );
    for skipped in &report.skipped {
        render_skipped(&mut out, skipped);
    }
    out
}

fn render_group(
    out: &mut String,
    index: usize,
    group: &DuplicateGroup,
    normalization: mct_index::Normalization,
) {
    let ranges: Vec<String> = group.members.iter().map(member_location).collect();
    let policy = match normalization {
        mct_index::Normalization::Broad => "identifiers -> $ID, literals -> $LIT",
        mct_index::Normalization::Consistent => {
            "consistent identifier spelling, literals -> $LIT, interpolation structure retained"
        }
        mct_index::Normalization::Exact => {
            "exact token text, interpolation structure retained; trivia ignored"
        }
    };
    let evidence = format!(
        "members share an identical normalized token sequence ({policy}); inspect {}",
        ranges.join(", ")
    );
    out.push_str("  - ");
    number_inline(out, "index", index);
    field(out, 4, "language", &group.language);
    number(out, 4, "member_count", group.members.len());
    number(out, 4, "line_count", group.line_count);
    number(out, 4, "token_count", group.token_count);
    field(out, 4, "evidence", &evidence);
    out.push_str("    verdict:\n");
    field(out, 6, "level", group.verdict.level.as_str());
    let reasons: Vec<String> = group
        .verdict
        .reasons
        .iter()
        .map(|reason| reason.as_str().to_string())
        .collect();
    out.push_str("      ");
    inline_array(out, "reasons", &reasons);
    out.push_str("    priority:\n");
    field(out, 6, "level", group.priority.level.as_str());
    field(out, 6, "call_shape", group.priority.call_shape.as_str());
    field(out, 6, "reason", &group.priority.reason);
    render_alignment(out, &group.evidence);
    render_context(out, &group.evidence.context);
    render_subgroups(out, group);
    let _ = writeln!(
        out,
        "    members[{}]{{path,range,symbol,kind,language,lines,tokens,excerpt}}:",
        group.members.len()
    );
    for member in &group.members {
        let row = [
            quote_if_needed(&member.path),
            format!("{}-{}", member.start_line, member.end_line),
            quote_if_needed(&member.symbol_name),
            quote_if_needed(&member.symbol_kind),
            quote_if_needed(&member.language),
            member.line_count.to_string(),
            member.token_count.to_string(),
            quote_if_needed(&member.excerpt.join("\n")),
        ];
        let _ = writeln!(out, "      {}", row.join(","));
    }
}

/// The group's aligned evidence: identifier pattern, differing positions per
/// kind, warnings, and the listed distinct differences. Per-member values and
/// lines are joined with ` / ` in member order.
fn render_alignment(out: &mut String, evidence: &GroupEvidence) {
    out.push_str("    alignment:\n");
    field(
        out,
        6,
        "identifier_pattern",
        evidence.identifier_pattern.as_str(),
    );
    out.push_str("      types:\n");
    field(out, 8, "compatibility", "unknown");
    number(
        out,
        8,
        "compared_positions",
        evidence.types.compared_positions,
    );
    number(
        out,
        8,
        "primitive_spelling_conflicts",
        evidence.types.primitive_spelling_conflicts,
    );
    let _ = writeln!(
        out,
        "        conflicts[{}]{{values,lines}}:",
        evidence.types.conflicts.len()
    );
    for conflict in &evidence.types.conflicts {
        let lines: Vec<String> = conflict.lines.iter().map(u32::to_string).collect();
        let _ = writeln!(
            out,
            "          {},{}",
            quote_if_needed(&conflict.values.join(" / ")),
            quote_if_needed(&lines.join(" / "))
        );
    }
    // An empty TOON object is its key with no nested fields.
    out.push_str("      differing_positions:\n");
    for (kind, count) in &evidence.differing_positions {
        number(out, 8, kind.as_str(), *count);
    }
    number(
        out,
        6,
        "distinct_differences",
        evidence.distinct_differences,
    );
    number(
        out,
        6,
        "undeclared_name_positions",
        evidence.undeclared_name_positions,
    );
    let _ = writeln!(
        out,
        "      warnings[{}]{{code,detail}}:",
        evidence.warnings.len()
    );
    for warning in &evidence.warnings {
        let _ = writeln!(
            out,
            "        {},{}",
            quote_if_needed(warning.code.as_str()),
            quote_if_needed(&warning.detail)
        );
    }
    let _ = writeln!(
        out,
        "      differences[{}]{{kind,occurrences,values,lines}}:",
        evidence.differences.len()
    );
    for difference in &evidence.differences {
        let lines: Vec<String> = difference.lines.iter().map(u32::to_string).collect();
        let _ = writeln!(
            out,
            "        {},{},{},{}",
            quote_if_needed(difference.kind.as_str()),
            difference.occurrences,
            quote_if_needed(&difference.values.join(" / ")),
            quote_if_needed(&lines.join(" / "))
        );
    }
}

/// Decorators and enclosing constructs, apart from the aligned tokens: one row
/// per aspect, with the shared value or the per-member values joined by ` / `.
fn render_context(out: &mut String, context: &ContextEvidence) {
    out.push_str("    context:\n");
    field(out, 6, "status", context.status.as_str());
    let _ = writeln!(
        out,
        "      aspects[{}]{{aspect,differs,values}}:",
        context.aspects.len()
    );
    for aspect in &context.aspects {
        let _ = writeln!(
            out,
            "        {},{},{}",
            aspect.aspect.as_str(),
            aspect.differs,
            quote_if_needed(&aspect.values.join(" / "))
        );
    }
}

/// The evidence-based partition of the group's members. `members` are 1-based
/// positions in the group's member table; `locations` name the same members.
fn render_subgroups(out: &mut String, group: &DuplicateGroup) {
    let subgroups = &group.subgroups;
    out.push_str("    subgroups:\n");
    field(out, 6, "status", subgroups.status.as_str());
    let basis: Vec<String> = subgroups
        .basis
        .iter()
        .map(|basis| basis.as_str().to_string())
        .collect();
    out.push_str("      ");
    inline_array(out, "basis", &basis);
    number(out, 6, "count", subgroups.count);
    number(out, 6, "separating_items", subgroups.separating_items);
    let _ = writeln!(out, "      all_distinct: {}", subgroups.all_distinct);
    number(
        out,
        6,
        "omitted_subgroups",
        if subgroups.all_distinct || subgroups.status != SubgroupStatus::Split {
            0
        } else {
            subgroups.count.saturating_sub(subgroups.subgroups.len())
        },
    );
    let _ = writeln!(
        out,
        "      rows[{}]{{id,size,members,locations,values}}:",
        subgroups.subgroups.len()
    );
    for row in &subgroups.subgroups {
        let indices: Vec<String> = row
            .members
            .iter()
            .map(|index| (index + 1).to_string())
            .collect();
        let locations: Vec<String> = row
            .members
            .iter()
            .filter_map(|&index| group.members.get(index))
            .map(|member| format!("{}:{}", member.path, member.start_line))
            .collect();
        let _ = writeln!(
            out,
            "        {},{},{},{},{}",
            row.id,
            row.size,
            quote_if_needed(&indices.join(" ")),
            quote_if_needed(&locations.join(" / ")),
            quote_if_needed(&row.values.join("; "))
        );
    }
}

fn render_skipped(out: &mut String, skipped: &SkippedCandidate) {
    let range = match skipped.end_line {
        Some(end) => format!("{}-{end}", skipped.start_line),
        None => format!("{}-?", skipped.start_line),
    };
    let row = [
        quote_if_needed(&skipped.path),
        range,
        quote_if_needed(&skipped.symbol_name),
        quote_if_needed(&skipped.language),
        quote_if_needed(skipped.reason.as_str()),
        quote_if_needed(&skipped.detail),
    ];
    let _ = writeln!(out, "  {}", row.join(","));
}

/// `path:start-end`, the form the evidence line lists for inspection.
fn member_location(member: &DuplicateMember) -> String {
    format!("{}:{}-{}", member.path, member.start_line, member.end_line)
}

fn field(out: &mut String, indent: usize, key: &str, value: &str) {
    let _ = writeln!(out, "{:indent$}{key}: {}", "", quote_if_needed(value));
}

fn number(out: &mut String, indent: usize, key: &str, value: usize) {
    let _ = writeln!(out, "{:indent$}{key}: {value}", "");
}

/// A number on the same line as a list item's hyphen.
fn number_inline(out: &mut String, key: &str, value: usize) {
    let _ = writeln!(out, "{key}: {value}");
}

/// `key[N]: a,b,c`, or `key[0]:` when empty.
fn inline_array(out: &mut String, key: &str, values: &[String]) {
    let _ = write!(out, "{key}[{}]:", values.len());
    if !values.is_empty() {
        let cells: Vec<String> = values.iter().map(|value| quote_if_needed(value)).collect();
        let _ = write!(out, " {}", cells.join(","));
    }
    out.push('\n');
}

/// Quotes `value` when TOON would otherwise misread it: empty, leading or
/// trailing whitespace, the comma delimiter, a colon, quotes, backslash,
/// brackets or braces, a control character, a leading hyphen, or text that
/// reads as a number, boolean or null. Escapes `\\`, `\"`, `\n`, `\r`, `\t`;
/// any other control character becomes U+FFFD.
fn quote_if_needed(value: &str) -> String {
    if !needs_quotes(value) {
        return value.to_string();
    }
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for c in value.chars() {
        match c {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            c if c.is_control() => quoted.push('\u{FFFD}'),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

fn needs_quotes(value: &str) -> bool {
    value.is_empty()
        || value.trim() != value
        || matches!(value, "true" | "false" | "null")
        || value.starts_with('-')
        || value.parse::<f64>().is_ok()
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, ',' | ':' | '"' | '\\' | '[' | ']' | '{' | '}'))
}

#[cfg(test)]
mod tests {
    // Test code: hand-built reports and string assertions, no repo input.
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use mct_index::{
        AlignedDifference, CallShape, ContextAspect, ContextComparison, ContextStatus,
        DifferenceKind, EvidenceWarning, GeneratedCodeHandling, GroupPriority, GroupSubgroups,
        GroupVerdict, IdentifierPattern, PriorityLevel, Subgroup, SubgroupBasis, VerdictCounts,
        VerdictLevel, VerdictReason, WarningCode,
    };

    use super::*;

    fn member(path: &str, start: u32, end: u32, name: &str, excerpt: &[&str]) -> DuplicateMember {
        DuplicateMember {
            path: path.to_string(),
            start_line: start,
            end_line: end,
            symbol_name: name.to_string(),
            symbol_kind: "function".to_string(),
            language: "rust".to_string(),
            line_count: 8,
            token_count: 52,
            excerpt: excerpt.iter().map(|line| (*line).to_string()).collect(),
        }
    }

    fn empty_report() -> DuplicateReport {
        DuplicateReport {
            total_groups: 0,
            demoted_groups: 0,
            verdict_counts: VerdictCounts::default(),
            split_groups: 0,
            index_syntax_errors: 0,
            index_read_failures: 0,
            has_indexing_record: false,
            groups: Vec::new(),
            skipped: Vec::new(),
            complete: true,
            options: DuplicateOptions::default(),
            candidates_analyzed: 0,
            below_threshold: 0,
            supported_languages: Vec::new(),
            unsupported_languages: Vec::new(),
            generated_code: GeneratedCodeHandling::ConfiguredExclusionsOnly,
        }
    }

    fn group_report() -> DuplicateReport {
        let mut report = empty_report();
        report.total_groups = 1;
        report.verdict_counts.suspicious = 1;
        report.supported_languages = vec!["python".to_string(), "rust".to_string()];
        report.unsupported_languages = vec!["go".to_string()];
        report.candidates_analyzed = 5;
        report.below_threshold = 2;
        report.groups = vec![DuplicateGroup {
            members: vec![
                member(
                    "src/a.rs",
                    10,
                    20,
                    "load",
                    &[
                        "    fn load(name: &str, flag: bool) -> Result<u8, String> {",
                        "        let x = \"a,b\";",
                        "    }",
                    ],
                ),
                member("src/b.rs", 3, 13, "read", &["fn read() {"]),
            ],
            language: "rust".to_string(),
            line_count: 8,
            token_count: 52,
            priority: GroupPriority {
                level: PriorityLevel::Normal,
                call_shape: CallShape::Other,
                reason: "every member's body has a statement other than a single flat call"
                    .to_string(),
            },
            evidence: GroupEvidence {
                types: mct_index::TypeEvidence::default(),
                identifier_pattern: IdentifierPattern::OneToOne,
                differing_positions: vec![
                    (DifferenceKind::Declaration, 1),
                    (DifferenceKind::LiteralCategory, 1),
                ],
                distinct_differences: 2,
                undeclared_name_positions: 0,
                differences: vec![
                    AlignedDifference {
                        kind: DifferenceKind::Declaration,
                        occurrences: 1,
                        values: vec!["load".to_string(), "read".to_string()],
                        lines: vec![10, 3],
                    },
                    AlignedDifference {
                        kind: DifferenceKind::LiteralCategory,
                        occurrences: 1,
                        values: vec!["1 (number)".to_string(), "\"a,b\" (string)".to_string()],
                        lines: vec![12, 5],
                    },
                ],
                context: ContextEvidence {
                    status: ContextStatus::Differs,
                    aspects: vec![
                        ContextComparison {
                            aspect: ContextAspect::Decorators,
                            differs: true,
                            values: vec!["#[inline]".to_string(), "(none)".to_string()],
                        },
                        ContextComparison {
                            aspect: ContextAspect::Enclosing,
                            differs: false,
                            values: vec!["impl".to_string()],
                        },
                    ],
                },
                warnings: vec![EvidenceWarning {
                    code: WarningCode::LiteralCategoriesDiffer,
                    detail: "1 aligned literals change category".to_string(),
                }],
            },
            verdict: GroupVerdict {
                level: VerdictLevel::Suspicious,
                reasons: vec![
                    VerdictReason::LiteralCategoriesDiffer,
                    VerdictReason::DecoratorsDiffer,
                ],
            },
            subgroups: GroupSubgroups {
                status: SubgroupStatus::Split,
                basis: vec![
                    SubgroupBasis::LiteralCategory,
                    SubgroupBasis::IdentifierPattern,
                ],
                count: 2,
                separating_items: 2,
                all_distinct: true,
                subgroups: Vec::new(),
            },
        }];
        report
    }

    fn partial_report() -> DuplicateReport {
        let mut report = empty_report();
        report.complete = false;
        report.supported_languages = vec!["rust".to_string()];
        report.skipped = vec![
            SkippedCandidate {
                path: "a.rs".to_string(),
                start_line: 4,
                end_line: Some(30),
                symbol_name: "f".to_string(),
                language: "rust".to_string(),
                reason: SkipReason::StaleLocation,
                detail: "file changed since indexing".to_string(),
            },
            SkippedCandidate {
                path: "b.rs".to_string(),
                start_line: 9,
                end_line: None,
                symbol_name: "g".to_string(),
                language: "rust".to_string(),
                reason: SkipReason::InvalidRange,
                detail: "missing end line".to_string(),
            },
            SkippedCandidate {
                path: "c.rs".to_string(),
                start_line: 1,
                end_line: Some(9),
                symbol_name: "h".to_string(),
                language: "rust".to_string(),
                reason: SkipReason::ParseError,
                detail: "syntax error at line 5: unexpected \"}\", expected `;`".to_string(),
            },
            SkippedCandidate {
                path: "d.rs".to_string(),
                start_line: 2,
                end_line: Some(12),
                symbol_name: "i".to_string(),
                language: "rust".to_string(),
                reason: SkipReason::StaleLocation,
                detail: "file changed since indexing".to_string(),
            },
        ];
        report
    }

    #[test]
    fn empty_report_is_valid_complete_output_with_zero_groups() {
        let expected = "\
notice: Groups below are CANDIDATE duplicates for human review. Identical normalized structure does not prove that the functions are redundant or behave the same.
status: complete
options:
  min_lines: 6
  min_tokens: 40
  normalization: broad
  ranking: lines
  offset: 0
  excerpts: included
  trivial_calls: demote
index_coverage: unknown
coverage_scope: whole index at last indexing; new unindexed files are not discovered
index_syntax_errors: 0
index_read_failures: 0
supported_languages[0]:
unsupported_languages[0]:
generated_code: generated code is excluded only by the project's configured exclusion rules; no generated-code marker is detected
alignment_scope: \"per group, the original identifier and literal tokens at aligned positions; roles are syntactic and nothing is resolved; differences are evidence, not verdicts\"
alignment_not_evaluated[6]: binding and scope resolution,type compatibility,effects of decorators and enclosing conditions,effects of calls in call-only bodies,partial-block repetition,behavioral equivalence (verdicts are heuristic explanations and prove nothing)
context_scope: \"per group, each member's decorators or outer attributes and enclosing constructs as text; compared, never resolved; a difference is a warning, not a verdict\"
priority_scope: \"per group, whether every member's body is only flat calls, read from syntax; callee names are never considered; demote ranks such groups last, keep leaves the ranking order; no group is removed\"
verdict_scope: \"per reported group, REAL, SUSPICIOUS or FALSE_POSITIVE from a fixed rule table over the alignment, types, context and priority evidence, with every rule that fired as a reason; heuristic explanations, not proof; REAL means no difference beyond a consistent renaming in the same context; no current rule emits FALSE_POSITIVE; verdicts never remove or reorder groups; counts cover the reported groups only\"
subgroups_scope: \"per group, members that agree on every differing member, callee and type text (names a member declares or takes as a parameter excluded), literal category and identifier pattern; decorators and enclosing constructs do not separate; an extra view, membership, totals and pagination are unchanged; not a verdict\"
candidates_analyzed: 0
below_threshold: 0
total_groups: 0
demoted_groups: 0
verdict_counts:
  real: 0
  suspicious: 0
  false_positive: 0
split_groups: 0
omitted_groups: 0
total_skipped: 0
skipped_by_reason:
  read-error: 0
  parse-error: 0
  stale-location: 0
  invalid-range: 0
groups[0]:
skipped[0]{path,range,symbol,language,reason,detail}:
";
        assert_eq!(render_toon(&empty_report()), expected);
    }

    #[test]
    fn groups_render_header_evidence_and_member_rows_in_report_order() {
        let out = render_toon(&group_report());
        assert!(
            out.contains("supported_languages[2]: python,rust\n"),
            "{out}"
        );
        assert!(out.contains("unsupported_languages[1]: go\n"), "{out}");
        assert!(out.contains("candidates_analyzed: 5\n"), "{out}");
        assert!(out.contains("below_threshold: 2\n"), "{out}");
        assert!(out.contains("total_groups: 1\n"), "{out}");
        assert!(out.contains("total_skipped: 0\n"), "{out}");
        assert!(
            out.contains("verdict_counts:\n  real: 0\n  suspicious: 1\n  false_positive: 0\n"),
            "{out}"
        );
        let expected = "\
groups[1]:
  - index: 1
    language: rust
    member_count: 2
    line_count: 8
    token_count: 52
    evidence: \"members share an identical normalized token sequence (identifiers -> $ID, literals -> $LIT); inspect src/a.rs:10-20, src/b.rs:3-13\"
    verdict:
      level: SUSPICIOUS
      reasons[2]: literal-categories-differ,decorators-differ
    priority:
      level: normal
      call_shape: other
      reason: every member's body has a statement other than a single flat call
    alignment:
      identifier_pattern: one-to-one
      types:
        compatibility: unknown
        compared_positions: 0
        primitive_spelling_conflicts: 0
        conflicts[0]{values,lines}:
      differing_positions:
        declaration: 1
        literal-category: 1
      distinct_differences: 2
      undeclared_name_positions: 0
      warnings[1]{code,detail}:
        literal-categories-differ,1 aligned literals change category
      differences[2]{kind,occurrences,values,lines}:
        declaration,1,load / read,10 / 3
        literal-category,1,\"1 (number) / \\\"a,b\\\" (string)\",12 / 5
    context:
      status: differs
      aspects[2]{aspect,differs,values}:
        decorators,true,\"#[inline] / (none)\"
        enclosing,false,impl
    subgroups:
      status: split
      basis[2]: literal-category,identifier-pattern
      count: 2
      separating_items: 2
      all_distinct: true
      omitted_subgroups: 0
      rows[0]{id,size,members,locations,values}:
    members[2]{path,range,symbol,kind,language,lines,tokens,excerpt}:
      src/a.rs,10-20,load,function,rust,8,52,\"    fn load(name: &str, flag: bool) -> Result<u8, String> {\\n        let x = \\\"a,b\\\";\\n    }\"
      src/b.rs,3-13,read,function,rust,8,52,\"fn read() {\"
skipped[0]{path,range,symbol,language,reason,detail}:
";
        assert!(out.ends_with(expected), "{out}");
        assert!(out.contains("status: complete\n"), "{out}");
        assert!(!out.contains("partial_reason"), "{out}");
    }

    #[test]
    fn partial_report_lists_every_reason_count_and_each_skipped_row() {
        let out = render_toon(&partial_report());
        assert!(out.contains("status: partial\n"), "{out}");
        assert!(out.contains("partial_reason: "), "{out}");
        assert!(out.contains("total_groups: 0\n"), "{out}");
        assert!(out.contains("total_skipped: 4\n"), "{out}");
        let counts = "\
skipped_by_reason:
  read-error: 0
  parse-error: 1
  stale-location: 2
  invalid-range: 1
";
        assert!(out.contains(counts), "{out}");
        let rows = "\
skipped[4]{path,range,symbol,language,reason,detail}:
  a.rs,4-30,f,rust,stale-location,file changed since indexing
  b.rs,9-?,g,rust,invalid-range,missing end line
  c.rs,1-9,h,rust,parse-error,\"syntax error at line 5: unexpected \\\"}\\\", expected `;`\"
  d.rs,2-12,i,rust,stale-location,file changed since indexing
";
        assert!(out.ends_with(rows), "{out}");
    }

    #[test]
    fn rendering_is_deterministic_and_keeps_the_reports_order() {
        let report = group_report();
        assert_eq!(render_toon(&report), render_toon(&report.clone()));

        let mut reordered = report.clone();
        reordered.groups[0].members.reverse();
        let out = render_toon(&reordered);
        let b = out.find("src/b.rs,3-13").unwrap();
        let a = out.find("src/a.rs,10-20").unwrap();
        assert!(b < a, "members must follow the report's order: {out}");
    }

    #[test]
    fn subgroup_rows_name_one_based_members_their_locations_and_omissions() {
        let mut report = group_report();
        report.split_groups = 1;
        report.groups[0].subgroups = GroupSubgroups {
            status: SubgroupStatus::Split,
            basis: vec![SubgroupBasis::Callee],
            count: 3,
            separating_items: 1,
            all_distinct: false,
            subgroups: vec![
                Subgroup {
                    id: 1,
                    size: 2,
                    members: vec![0, 1],
                    values: vec!["callee=load".into(), "member=a,b".into()],
                },
                Subgroup {
                    id: 2,
                    size: 1,
                    members: vec![1],
                    values: vec!["callee=read".into()],
                },
            ],
        };
        let out = render_toon(&report);
        assert!(out.contains("split_groups: 1\n"), "{out}");
        let expected = "    subgroups:
      status: split
      basis[1]: callee
      count: 3
      separating_items: 1
      all_distinct: false
      omitted_subgroups: 1
      rows[2]{id,size,members,locations,values}:
        1,2,1 2,\"src/a.rs:10 / src/b.rs:3\",\"callee=load; member=a,b\"
        2,1,\"2\",\"src/b.rs:3\",callee=read
";
        assert!(out.contains(expected), "{out}");
    }

    #[test]
    fn quoting_follows_the_toon_rules() {
        for plain in ["abc", "src/a.rs", "a b", "rust", "$ID", "a-b", "x.y"] {
            assert_eq!(quote_if_needed(plain), plain);
        }
        for (raw, quoted) in [
            ("", "\"\""),
            ("a,b", "\"a,b\""),
            ("a: b", "\"a: b\""),
            ("say \"hi\"", "\"say \\\"hi\\\"\""),
            ("back\\slash", "\"back\\\\slash\""),
            (" lead", "\" lead\""),
            ("trail ", "\"trail \""),
            ("[x]", "\"[x]\""),
            ("{x}", "\"{x}\""),
            ("true", "\"true\""),
            ("false", "\"false\""),
            ("null", "\"null\""),
            ("42", "\"42\""),
            ("-7", "\"-7\""),
            ("3.14", "\"3.14\""),
            ("05", "\"05\""),
            ("-x", "\"-x\""),
            ("a\nb", "\"a\\nb\""),
            ("a\r\nb\tc", "\"a\\r\\nb\\tc\""),
            ("a\u{1}b", "\"a\u{FFFD}b\""),
        ] {
            assert_eq!(quote_if_needed(raw), quoted, "value {raw:?}");
        }
    }

    #[test]
    fn excerpt_with_commas_quotes_and_colons_stays_on_one_row() {
        let out = render_toon(&group_report());
        let row = out
            .lines()
            .find(|line| line.trim_start().starts_with("src/a.rs,10-20"))
            .unwrap();
        // The three excerpt lines are escaped into one quoted cell.
        assert!(
            row.contains("\\n        let x = \\\"a,b\\\";\\n    }\""),
            "{row}"
        );
        assert_eq!(
            out.lines().filter(|l| l.contains("src/a.rs,10-20")).count(),
            1
        );
    }

    #[test]
    fn empty_excerpt_is_an_explicit_empty_string() {
        let mut report = group_report();
        report.groups[0].members[1].excerpt.clear();
        let out = render_toon(&report);
        assert!(
            out.contains("src/b.rs,3-13,read,function,rust,8,52,\"\"\n"),
            "{out}"
        );
    }

    #[test]
    fn language_lists_with_special_characters_are_quoted() {
        let mut report = empty_report();
        report.supported_languages = vec!["c++".to_string(), "a,b".to_string()];
        let out = render_toon(&report);
        assert!(
            out.contains("supported_languages[2]: c++,\"a,b\"\n"),
            "{out}"
        );
    }

    #[test]
    fn threshold_parser_accepts_positive_integers_only() {
        assert_eq!(parse_threshold("6"), Ok(6));
        assert_eq!(parse_threshold(" 12 "), Ok(12));
        assert!(parse_threshold("0").is_err());
        assert!(parse_threshold("-3").is_err());
        assert!(parse_threshold("abc").is_err());
        assert!(parse_threshold("").is_err());
        assert!(parse_threshold("2.5").is_err());
    }

    #[test]
    fn skip_reason_order_covers_every_reason_once() {
        let names: Vec<&str> = SKIP_REASONS.iter().map(|r| r.as_str()).collect();
        assert_eq!(
            names,
            [
                "read-error",
                "parse-error",
                "stale-location",
                "invalid-range"
            ]
        );
        // Exhaustive match: a new variant breaks the build here, reminding the
        // author to extend SKIP_REASONS.
        for reason in SKIP_REASONS {
            match reason {
                SkipReason::ReadError
                | SkipReason::ParseError
                | SkipReason::StaleLocation
                | SkipReason::InvalidRange => {}
            }
        }
    }
}
