//! Candidate collection, tokenization, normalization and grouping for
//! [`super::find_duplicate_groups`].
//!
//! Flow: indexed functions and methods become candidates (languages without a
//! token source are only reported as unsupported); each file is read and
//! tokenized once; each candidate's tokens are normalized (identifiers and
//! literals replaced, keywords and punctuation kept) and bucketed by
//! `(language, sequence)` with exact sequence equality. Nothing here mutates
//! the index or a file, and nothing panics on repository content: a
//! per-candidate failure becomes a [`SkippedCandidate`], and only a failing
//! index query returns `Err`.

mod evidence;
mod file;
mod fingerprint;
mod verdict;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use mct_core::{LineRange, TokenSourceRegistry};

use super::{
    DuplicateGroup, DuplicateMember, DuplicateOptions, DuplicateReport, GeneratedCodeHandling,
    GroupEvidence, GroupPriority, GroupVerdict, PriorityLevel, SkipReason, SkippedCandidate,
    VerdictCounts, VerdictLevel, DUPLICATE_KIND_ALLOWLIST, MAX_EXCERPT_LINES,
    MAX_EXCERPT_LINE_CHARS,
};
use crate::{queries, Index, Result, SymbolListEntry};
use evidence::{MemberContext, MemberInput, Texts};
use fingerprint::Interner;

/// Identity of a grouped member: path, start, end and symbol name.
type MemberKey = (String, u32, u32, String);

pub(super) fn run(
    index: &Index,
    token_sources: &TokenSourceRegistry,
    options: DuplicateOptions,
) -> Result<DuplicateReport> {
    // Query indexed candidates directly: deleted and hidden files stay visible.
    let file_hashes = index.file_hashes()?;
    let entries = queries::list_duplicate_candidates(
        &index.conn,
        options.path.as_deref(),
        options.language.as_deref(),
    )?;
    let hashes: HashMap<String, String> = file_hashes.into_iter().collect();

    // Candidates per (file, language), so each file is read and tokenized once.
    let mut files: BTreeMap<(&str, &str), Vec<&SymbolListEntry>> = BTreeMap::new();
    let mut unsupported: BTreeSet<&str> = BTreeSet::new();
    for entry in &entries {
        if !DUPLICATE_KIND_ALLOWLIST.contains(&entry.kind.as_str()) {
            continue;
        }
        if options
            .language
            .as_ref()
            .is_some_and(|language| language != &entry.language)
            || options.path.as_deref().is_some_and(|path| {
                let path = path.trim_end_matches('/');
                entry.relative_path != path && !entry.relative_path.starts_with(&format!("{path}/"))
            })
        {
            continue;
        }
        // A path the current rules exclude may still have rows (rules edited
        // since the last full reindex): it is not part of the project.
        if index.exclude.is_excluded(&entry.relative_path) {
            continue;
        }
        if token_sources.for_language(&entry.language).is_none() {
            unsupported.insert(&entry.language);
            continue;
        }
        files
            .entry((&entry.relative_path, &entry.language))
            .or_default()
            .push(entry);
    }

    let structural = options.normalization != super::Normalization::Broad;
    let mut analysis = Analysis::new(options.clone());
    for ((path, language), candidates) in &files {
        let Some(token_source) = token_sources.for_language(language) else {
            continue;
        };
        match file::load(
            index.root(),
            path,
            hashes.get(*path).map(String::as_str),
            token_source.as_ref(),
            structural,
        ) {
            Ok(loaded) => analysis.analyze_file(&loaded, candidates),
            Err(failure) => {
                for candidate in candidates {
                    analysis.skip(candidate, failure.reason, failure.detail.clone());
                }
            }
        }
    }

    let (mut report, texts, mut inputs) = analysis.into_report(
        token_sources,
        unsupported.into_iter().map(str::to_string).collect(),
    );
    // Whole-index diagnostics: failures may have no symbol row at all.
    let mut statement = index.conn.prepare("SELECT relative_path, issue_kind FROM index_issues WHERE issue_kind IN ('syntax_error', 'read_failure')")?;
    let issues = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for issue in issues {
        let (path, kind) = issue?;
        if index.exclude.is_excluded(&path) {
            continue;
        }
        if kind == "syntax_error" {
            report.index_syntax_errors += 1;
        } else {
            report.index_read_failures += 1;
        }
    }
    if report.options.excerpts {
        hydrate_excerpts(index, &hashes, &mut report);
    }
    // Priority needs every group, so it runs before pagination and after
    // late excerpt failures have removed members.
    for group in &mut report.groups {
        let bodies: Vec<_> = group
            .members
            .iter()
            .map(|member| {
                inputs
                    .get(&(
                        member.path.clone(),
                        member.start_line,
                        member.end_line,
                        member.symbol_name.clone(),
                    ))
                    .and_then(|input| input.body)
            })
            .collect();
        group.priority = evidence::evaluate_priority(&bodies, report.options.trivial_calls);
    }
    // Stable: demoted groups keep their ranking order among themselves.
    report
        .groups
        .sort_by_key(|group| group.priority.level == PriorityLevel::Demoted);
    report.demoted_groups = report
        .groups
        .iter()
        .filter(|group| group.priority.level == PriorityLevel::Demoted)
        .count();
    report.total_groups = report.groups.len();
    report.groups = report
        .groups
        .into_iter()
        .skip(report.options.offset)
        .take(report.options.limit.unwrap_or(usize::MAX))
        .collect();
    for group in &mut report.groups {
        let members: Option<Vec<MemberInput>> = group
            .members
            .iter()
            .map(|member| {
                inputs.remove(&(
                    member.path.clone(),
                    member.start_line,
                    member.end_line,
                    member.symbol_name.clone(),
                ))
            })
            .collect();
        let members = members.unwrap_or_default();
        let slots: Vec<&[evidence::Slot]> = members
            .iter()
            .map(|member| member.slots.as_slice())
            .collect();
        let contexts: Vec<Option<&MemberContext>> = members
            .iter()
            .map(|member| member.context.as_ref())
            .collect();
        group.evidence = evidence::evaluate(&slots, &texts);
        let (context, warnings) = evidence::evaluate_context(&contexts);
        group.evidence.context = context;
        group.evidence.warnings.extend(warnings);
        // After all evidence: the verdict only reads it, never changes it.
        group.verdict = verdict::classify(&group.evidence, &group.priority);
        let count = match group.verdict.level {
            VerdictLevel::Real => &mut report.verdict_counts.real,
            VerdictLevel::Suspicious => &mut report.verdict_counts.suspicious,
            VerdictLevel::FalsePositive => &mut report.verdict_counts.false_positive,
        };
        *count += 1;
    }
    report.has_indexing_record = index.conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM index_meta WHERE key = 'last_indexed_at')",
        [],
        |row| row.get(0),
    )?;
    Ok(report)
}

/// Running state of one analysis.
struct Analysis {
    options: DuplicateOptions,
    interner: Interner,
    candidates_analyzed: usize,
    below_threshold: usize,
    skipped: Vec<SkippedCandidate>,
    /// Members per `(language, normalized sequence)`; the key is compared in
    /// full, so a hash collision can never merge two different sequences.
    buckets: HashMap<(String, Vec<usize>), usize>,
    members: Vec<Vec<DuplicateMember>>,
    /// Evidence input, parallel to `members`.
    inputs: Vec<Vec<MemberInput>>,
}

impl Analysis {
    fn new(options: DuplicateOptions) -> Self {
        Self {
            options,
            interner: Interner::default(),
            candidates_analyzed: 0,
            below_threshold: 0,
            skipped: Vec::new(),
            buckets: HashMap::new(),
            members: Vec::new(),
            inputs: Vec::new(),
        }
    }

    fn skip(&mut self, candidate: &SymbolListEntry, reason: SkipReason, detail: String) {
        self.skipped.push(SkippedCandidate {
            path: candidate.relative_path.clone(),
            start_line: candidate.line,
            end_line: candidate.end_line,
            symbol_name: candidate.name.clone(),
            language: candidate.language.clone(),
            reason,
            detail,
        });
    }

    fn analyze_file(&mut self, loaded: &file::LoadedFile, candidates: &[&SymbolListEntry]) {
        let lines: Vec<&str> = loaded.source.lines().collect();
        let mut touched: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for candidate in candidates {
            match check_range(candidate, lines.len(), &loaded.error_lines) {
                Ok((start, end)) => {
                    if let Some((bucket, member)) =
                        self.analyze_candidate(candidate, loaded, start, end)
                    {
                        touched.entry(bucket).or_default().push(member);
                    }
                }
                Err((reason, detail)) => self.skip(candidate, reason, detail),
            }
        }
        // Once a bucket is a group, reuse this file's loaded source. Earlier
        // members in other files are fetched lazily after grouping.
        if self.options.excerpts {
            for (bucket, positions) in touched {
                let members = &mut self.members[bucket];
                if members.len() < 2 {
                    continue;
                }
                for position in positions {
                    let member = &mut members[position];
                    member.excerpt = excerpt(&lines, member.start_line, member.end_line);
                }
            }
        }
    }

    fn analyze_candidate(
        &mut self,
        candidate: &SymbolListEntry,
        loaded: &file::LoadedFile,
        start: u32,
        end: u32,
    ) -> Option<(usize, usize)> {
        let span = if let Some(spans) = &loaded.callables {
            let first = spans.partition_point(|span| span.lines.start < start);
            let mut matching = spans[first..]
                .iter()
                .take_while(|span| span.lines.start == start)
                .filter(|span| {
                    span.name == candidate.name
                        && span.lines.start == start
                        && span.lines.end == end
                });
            match (matching.next(), matching.next()) {
                (Some(span), None) => Some(span),
                _ => {
                    self.skip(
                        candidate,
                        SkipReason::InvalidRange,
                        "no unique callable boundary matches the indexed symbol".to_string(),
                    );
                    return None;
                }
            }
        } else {
            None
        };
        let fingerprint = fingerprint::fingerprint(
            &loaded.tokens,
            start,
            end,
            span.map(|span| &span.bytes),
            &mut self.interner,
            self.options.normalization,
            &candidate.name,
        );
        self.candidates_analyzed += 1;
        // A range without any token (blank or comment-only) has nothing to
        // compare, whatever the thresholds are.
        if fingerprint.token_count == 0
            || fingerprint.line_count < self.options.min_lines
            || fingerprint.token_count < self.options.min_tokens
        {
            self.below_threshold += 1;
            return None;
        }
        let member = DuplicateMember {
            path: candidate.relative_path.clone(),
            start_line: start,
            end_line: end,
            symbol_name: candidate.name.clone(),
            symbol_kind: candidate.kind.clone(),
            language: candidate.language.clone(),
            line_count: fingerprint.line_count,
            token_count: fingerprint.token_count,
            excerpt: Vec::new(),
        };
        let bucket = *self
            .buckets
            .entry((candidate.language.clone(), fingerprint.sequence))
            .or_insert_with(|| {
                self.members.push(Vec::new());
                self.inputs.push(Vec::new());
                self.members.len() - 1
            });
        self.members[bucket].push(member);
        self.inputs[bucket].push(MemberInput {
            slots: fingerprint.slots,
            context: span.map(|span| MemberContext {
                decorators: span.decorators.clone(),
                enclosing: span.enclosing.clone(),
            }),
            body: span.and_then(|span| span.body),
        });
        Some((bucket, self.members[bucket].len() - 1))
    }

    fn into_report(
        self,
        token_sources: &TokenSourceRegistry,
        unsupported_languages: Vec<String>,
    ) -> (DuplicateReport, Texts, HashMap<MemberKey, MemberInput>) {
        let mut inputs = HashMap::new();
        let mut groups: Vec<DuplicateGroup> = self
            .members
            .into_iter()
            .zip(self.inputs)
            .filter_map(|(members, member_inputs)| {
                if members.len() < 2 {
                    return None;
                }
                for (member, member_input) in members.iter().zip(member_inputs) {
                    inputs.insert(
                        (
                            member.path.clone(),
                            member.start_line,
                            member.end_line,
                            member.symbol_name.clone(),
                        ),
                        member_input,
                    );
                }
                let language = members.first()?.language.clone();
                Some(build_group(language, members))
            })
            .collect();
        sort_groups(&mut groups, self.options.ranking);

        let mut skipped = self.skipped;
        skipped.sort_by(|a, b| {
            (&a.path, a.start_line, a.end_line, &a.symbol_name, a.reason).cmp(&(
                &b.path,
                b.start_line,
                b.end_line,
                &b.symbol_name,
                b.reason,
            ))
        });

        let total_groups = groups.len();
        let report = DuplicateReport {
            total_groups,
            demoted_groups: 0,
            verdict_counts: VerdictCounts::default(),
            index_syntax_errors: 0,
            index_read_failures: 0,
            has_indexing_record: false,
            groups,
            complete: skipped.is_empty(),
            skipped,
            options: self.options,
            candidates_analyzed: self.candidates_analyzed,
            below_threshold: self.below_threshold,
            supported_languages: token_sources
                .language_ids()
                .into_iter()
                .map(str::to_string)
                .collect(),
            unsupported_languages,
            generated_code: GeneratedCodeHandling::ConfiguredExclusionsOnly,
        };
        (report, self.interner.texts, inputs)
    }
}

/// Fetch excerpts only for actual groups. Recheck hashes so a concurrent edit
/// never attaches newer text to older fingerprints. Failed members are removed
/// before ranking and pagination, and mark the report partial.
fn hydrate_excerpts(index: &Index, hashes: &HashMap<String, String>, report: &mut DuplicateReport) {
    let mut by_path: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
    for (group, entry) in report.groups.iter().enumerate() {
        for (member, entry) in entry.members.iter().enumerate() {
            if !entry.excerpt.is_empty() {
                continue;
            }
            by_path
                .entry(entry.path.clone())
                .or_default()
                .push((group, member));
        }
    }
    let mut failed = BTreeSet::new();
    for (path, positions) in by_path {
        match file::read_source(index.root(), &path, hashes.get(&path).map(String::as_str)) {
            Ok(source) => {
                let lines: Vec<_> = source.lines().collect();
                for (g, m) in positions {
                    let member = &mut report.groups[g].members[m];
                    member.excerpt = excerpt(&lines, member.start_line, member.end_line);
                }
            }
            Err(failure) => {
                for (g, m) in positions {
                    let member = &report.groups[g].members[m];
                    report.skipped.push(SkippedCandidate {
                        path: path.clone(),
                        start_line: member.start_line,
                        end_line: Some(member.end_line),
                        symbol_name: member.symbol_name.clone(),
                        language: member.language.clone(),
                        reason: failure.reason,
                        detail: format!("excerpt read: {}", failure.detail),
                    });
                }
                failed.insert(path);
            }
        }
    }
    if !failed.is_empty() {
        report.complete = false;
        report.groups = std::mem::take(&mut report.groups)
            .into_iter()
            .filter_map(|mut group| {
                group
                    .members
                    .retain(|member| !failed.contains(&member.path));
                (group.members.len() >= 2).then(|| build_group(group.language, group.members))
            })
            .collect();
        sort_groups(&mut report.groups, report.options.ranking);
        report.skipped.sort_by(|a, b| {
            (&a.path, a.start_line, &a.symbol_name).cmp(&(&b.path, b.start_line, &b.symbol_name))
        });
    }
}

/// Validates the indexed range against the file. Returns the 1-based inclusive
/// `(start, end)` lines, or why the candidate cannot be tokenized.
fn check_range(
    candidate: &SymbolListEntry,
    line_total: usize,
    error_lines: &[LineRange],
) -> std::result::Result<(u32, u32), (SkipReason, String)> {
    let invalid = |detail: &str| (SkipReason::InvalidRange, detail.to_string());
    let start = candidate.line;
    let Some(end) = candidate.end_line else {
        return Err(invalid("the index holds no end line"));
    };
    if start == 0 {
        return Err(invalid("start line is 0"));
    }
    if end < start {
        return Err(invalid("end line precedes start line"));
    }
    if usize::try_from(end).map_or(true, |end| end > line_total) {
        return Err(invalid("end line is past the end of the file"));
    }
    let overlapping = error_lines.iter().find(|range| {
        let (low, high) = (range.start.min(range.end), range.start.max(range.end));
        low <= end && high >= start
    });
    if let Some(range) = overlapping {
        return Err((
            SkipReason::ParseError,
            format!("syntax error at lines {}-{}", range.start, range.end),
        ));
    }
    Ok((start, end))
}

/// The first [`MAX_EXCERPT_LINES`] lines of `start..=end`, each cut to
/// [`MAX_EXCERPT_LINE_CHARS`] characters (on a character boundary).
fn excerpt(lines: &[&str], start: u32, end: u32) -> Vec<String> {
    let first = usize::try_from(start).map_or(0, |start| start.saturating_sub(1));
    let wanted = usize::try_from(end.saturating_sub(start).saturating_add(1))
        .map_or(MAX_EXCERPT_LINES, |count| count.min(MAX_EXCERPT_LINES));
    lines
        .iter()
        .skip(first)
        .take(wanted)
        .map(|line| line.chars().take(MAX_EXCERPT_LINE_CHARS).collect())
        .collect()
}

fn build_group(language: String, mut members: Vec<DuplicateMember>) -> DuplicateGroup {
    members.sort_by(|a, b| {
        (&a.path, a.start_line, a.end_line, &a.symbol_name).cmp(&(
            &b.path,
            b.start_line,
            b.end_line,
            &b.symbol_name,
        ))
    });
    DuplicateGroup {
        language,
        line_count: members.iter().map(|m| m.line_count).min().unwrap_or(0),
        token_count: members.first().map_or(0, |m| m.token_count),
        members,
        priority: GroupPriority::default(),
        evidence: GroupEvidence::default(),
        verdict: GroupVerdict::default(),
    }
}

/// `line_count × members.len()`, the order groups are reported in.
fn group_weight(group: &DuplicateGroup, ranking: super::DuplicateRanking) -> usize {
    match ranking {
        super::DuplicateRanking::Lines => group.line_count.saturating_mul(group.members.len()),
        super::DuplicateRanking::Tokens => group
            .token_count
            .saturating_mul(group.members.len().saturating_sub(1)),
    }
}

/// Tie-breaker between equally heavy groups: where the first member starts.
fn first_member_key(group: &DuplicateGroup) -> Option<(&str, u32, u32)> {
    group
        .members
        .first()
        .map(|member| (member.path.as_str(), member.start_line, member.end_line))
}

fn sort_groups(groups: &mut [DuplicateGroup], ranking: super::DuplicateRanking) {
    groups.sort_by(|a, b| {
        group_weight(b, ranking)
            .cmp(&group_weight(a, ranking))
            .then_with(|| first_member_key(a).cmp(&first_member_key(b)))
            .then_with(|| a.language.cmp(&b.language))
            .then_with(|| a.token_count.cmp(&b.token_count))
    });
}
