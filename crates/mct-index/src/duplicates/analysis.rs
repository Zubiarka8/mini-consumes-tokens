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

mod file;
mod fingerprint;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use mct_core::{LineRange, TokenSourceRegistry};

use super::{
    DuplicateGroup, DuplicateMember, DuplicateOptions, DuplicateReport, GeneratedCodeHandling,
    SkipReason, SkippedCandidate, DUPLICATE_KIND_ALLOWLIST, MAX_EXCERPT_LINES,
    MAX_EXCERPT_LINE_CHARS,
};
use crate::{Index, Result, SymbolListEntry};
use fingerprint::Interner;

pub(super) fn run(
    index: &Index,
    token_sources: &TokenSourceRegistry,
    options: DuplicateOptions,
) -> Result<DuplicateReport> {
    let entries = index.list_symbols_all(None, None)?;
    let hashes: HashMap<String, String> = index.file_hashes()?.into_iter().collect();

    // Candidates per (file, language), so each file is read and tokenized once.
    let mut files: BTreeMap<(&str, &str), Vec<&SymbolListEntry>> = BTreeMap::new();
    let mut unsupported: BTreeSet<&str> = BTreeSet::new();
    for entry in &entries {
        if !DUPLICATE_KIND_ALLOWLIST.contains(&entry.kind.as_str()) {
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

    let mut analysis = Analysis::new(options);
    for ((path, language), candidates) in &files {
        let Some(token_source) = token_sources.for_language(language) else {
            continue;
        };
        match file::load(
            index.root(),
            path,
            hashes.get(*path).map(String::as_str),
            token_source.as_ref(),
        ) {
            Ok(loaded) => analysis.analyze_file(&loaded, candidates),
            Err(failure) => {
                for candidate in candidates {
                    analysis.skip(candidate, failure.reason, failure.detail.clone());
                }
            }
        }
    }

    Ok(analysis.into_report(
        token_sources,
        unsupported.into_iter().map(str::to_string).collect(),
    ))
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
    buckets: HashMap<(String, Vec<usize>), Vec<DuplicateMember>>,
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
        for candidate in candidates {
            match check_range(candidate, lines.len(), &loaded.error_lines) {
                Ok((start, end)) => self.analyze_candidate(candidate, loaded, &lines, start, end),
                Err((reason, detail)) => self.skip(candidate, reason, detail),
            }
        }
    }

    fn analyze_candidate(
        &mut self,
        candidate: &SymbolListEntry,
        loaded: &file::LoadedFile,
        lines: &[&str],
        start: u32,
        end: u32,
    ) {
        let fingerprint = fingerprint::fingerprint(&loaded.tokens, start, end, &mut self.interner);
        self.candidates_analyzed += 1;
        // A range without any token (blank or comment-only) has nothing to
        // compare, whatever the thresholds are.
        if fingerprint.token_count == 0
            || fingerprint.line_count < self.options.min_lines
            || fingerprint.token_count < self.options.min_tokens
        {
            self.below_threshold += 1;
            return;
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
            excerpt: excerpt(lines, start, end),
        };
        self.buckets
            .entry((candidate.language.clone(), fingerprint.sequence))
            .or_default()
            .push(member);
    }

    fn into_report(
        self,
        token_sources: &TokenSourceRegistry,
        unsupported_languages: Vec<String>,
    ) -> DuplicateReport {
        let mut groups: Vec<DuplicateGroup> = self
            .buckets
            .into_iter()
            .filter(|(_, members)| members.len() >= 2)
            .map(|((language, _), members)| build_group(language, members))
            .collect();
        groups.sort_by(|a, b| {
            group_weight(b)
                .cmp(&group_weight(a))
                .then_with(|| first_member_key(a).cmp(&first_member_key(b)))
                .then_with(|| a.language.cmp(&b.language))
                .then_with(|| a.token_count.cmp(&b.token_count))
        });

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

        DuplicateReport {
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
        }
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
    }
}

/// `line_count × members.len()`, the order groups are reported in.
fn group_weight(group: &DuplicateGroup) -> usize {
    group.line_count.saturating_mul(group.members.len())
}

/// Tie-breaker between equally heavy groups: where the first member starts.
fn first_member_key(group: &DuplicateGroup) -> Option<(&str, u32, u32)> {
    group
        .members
        .first()
        .map(|member| (member.path.as_str(), member.start_line, member.end_line))
}
