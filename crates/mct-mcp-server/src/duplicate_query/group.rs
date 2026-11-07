//! Bounded evidence and references for one member window.
use super::{presentation::table, Row};
use std::ops::Range;

fn clipped(text: &str) -> String {
    let mut out: String = text.chars().take(160).collect();
    if text.chars().count() > 160 {
        out.push('…');
    }
    out
}

fn validity(row: &Row) -> &'static str {
    if row.resolution.held {
        "review-store-unreadable"
    } else if row.resolution.winner.is_none() {
        "no-saved-decision"
    } else if row.resolution.state() == mct_index::reviews::State::Unreviewed {
        "restored"
    } else if !row.finding.content_known {
        "content-unavailable"
    } else if row.resolution.needs_reevaluation() {
        "fingerprint-changed"
    } else {
        "current-fingerprint"
    }
}

pub(super) fn group_text(row: &Row, range: Range<usize>, details: bool) -> String {
    let g = row.group;
    let mut out = table(
        "group",
        &[
            "id",
            "language",
            "members",
            "lines",
            "tokens",
            "review",
            "actionable",
            "verdict",
            "priority",
            "review_validity",
        ],
        vec![vec![
            row.finding.identity.clone(),
            g.language.clone(),
            g.members.len().to_string(),
            g.line_count.to_string(),
            g.token_count.to_string(),
            row.resolution.effective_label().into(),
            row.actionable().to_string(),
            g.verdict.level.as_str().into(),
            g.priority.level.as_str().into(),
            validity(row).into(),
        ]],
    );
    out.push_str(&table(
        "members",
        &[
            "member_index",
            "path",
            "symbol",
            "kind",
            "start",
            "column",
            "end",
            "subgroup",
            "symbol_truncated",
        ],
        g.members[range.clone()]
            .iter()
            .enumerate()
            .map(|(i, m)| {
                vec![
                    (range.start + i + 1).to_string(),
                    m.path.clone(),
                    clipped(&m.symbol_name),
                    m.symbol_kind.clone(),
                    m.start_line.to_string(),
                    m.start_column.to_string(),
                    m.end_line.to_string(),
                    g.subgroups
                        .member_subgroups
                        .get(range.start + i)
                        .map_or("unknown".into(), usize::to_string),
                    (m.symbol_name.chars().count() > 160).to_string(),
                ]
            })
            .collect(),
    ));
    out.push_str(&format!("members_omitted: {}\nmember_offset: {}\nmember_indices: one-based-global\nevidence_omitted: {}\n", g.members.len()-range.len(), range.start, !details));
    if !details {
        return out;
    }
    out.push_str(&format!(
        "members_returned: {}\nmembers_remaining: {}\ntext_limit_chars: 160\n",
        range.len(),
        g.members.len() - range.end
    ));
    out.push_str(&format!("type_compatibility: unknown\nidentifier_pattern: {}\ncontext_status: {}\nsubgroup_status: {}\nsubgroup_count: {}\nsubgroups_all_distinct: {}\nsubgroup_rows_omitted: {}\ndifferences_unlisted: {}\nreview_fingerprint_basis: whole-files-and-member-ranges\nstored_review: {}\nreview_conflict: {}\n", g.evidence.identifier_pattern.as_str(), g.evidence.context.status.as_str(), g.subgroups.status.as_str(), g.subgroups.count, g.subgroups.all_distinct, if g.subgroups.all_distinct || g.subgroups.count <= 1 {0} else {g.subgroups.count.saturating_sub(g.subgroups.subgroups.len())}, g.evidence.distinct_differences.saturating_sub(g.evidence.differences.len()), row.resolution.state().value(), row.resolution.conflict));
    out.push_str(&table(
        "verdict_reasons",
        &["code"],
        g.verdict
            .reasons
            .iter()
            .map(|r| vec![r.as_str().into()])
            .collect(),
    ));
    out.push_str(&table(
        "priority",
        &["call_shape", "reason"],
        vec![vec![
            g.priority.call_shape.as_str().into(),
            clipped(&g.priority.reason),
        ]],
    ));
    out.push_str(&table(
        "warnings",
        &["code", "detail"],
        g.evidence
            .warnings
            .iter()
            .map(|w| vec![w.code.as_str().into(), clipped(&w.detail)])
            .collect(),
    ));
    out.push_str(&table(
        "differing_positions",
        &["kind", "count"],
        g.evidence
            .differing_positions
            .iter()
            .map(|(k, n)| vec![k.as_str().into(), n.to_string()])
            .collect(),
    ));
    let mut differences = Vec::new();
    let mut values = 0;
    for (d, difference) in g.evidence.differences.iter().enumerate() {
        values += difference.values.len();
        for member in range.clone() {
            if differences.len() >= 24 {
                break;
            }
            if let Some(value) = difference.values.get(member) {
                differences.push(vec![
                    (d + 1).to_string(),
                    difference.kind.as_str().into(),
                    difference.occurrences.to_string(),
                    (member + 1).to_string(),
                    difference.lines.get(member).unwrap_or(&0).to_string(),
                    clipped(value),
                ]);
            }
        }
    }
    out.push_str(&format!(
        "difference_values_omitted: {}\n",
        values.saturating_sub(differences.len())
    ));
    out.push_str(&table(
        "differences",
        &[
            "difference",
            "kind",
            "occurrences",
            "member_index",
            "line",
            "value",
        ],
        differences,
    ));
    out.push_str(&table(
        "types",
        &[
            "compared_positions",
            "primitive_spelling_conflicts",
            "compatibility",
        ],
        vec![vec![
            g.evidence.types.compared_positions.to_string(),
            g.evidence.types.primitive_spelling_conflicts.to_string(),
            "unknown".into(),
        ]],
    ));
    let mut conflicts = Vec::new();
    let mut conflict_values = 0;
    for conflict in &g.evidence.types.conflicts {
        conflict_values += conflict.values.len();
        for member in range.clone() {
            if conflicts.len() >= 24 {
                break;
            }
            if let Some(value) = conflict.values.get(member) {
                conflicts.push(vec![
                    (member + 1).to_string(),
                    conflict.lines.get(member).unwrap_or(&0).to_string(),
                    clipped(value),
                ]);
            }
        }
    }
    out.push_str(&format!(
        "type_conflict_values_omitted: {}\n",
        conflict_values.saturating_sub(conflicts.len())
    ));
    out.push_str(&table(
        "type_conflicts",
        &["member_index", "line", "value"],
        conflicts,
    ));
    let mut context = Vec::new();
    let mut context_values = 0;
    for aspect in &g.evidence.context.aspects {
        context_values += g.members.len();
        for member in range.clone().take(8) {
            if let Some(value) = aspect.values.get(if aspect.differs { member } else { 0 }) {
                context.push(vec![
                    aspect.aspect.as_str().into(),
                    aspect.differs.to_string(),
                    (member + 1).to_string(),
                    clipped(value),
                ]);
            }
        }
    }
    out.push_str(&format!(
        "context_values_omitted: {}\n",
        context_values.saturating_sub(context.len())
    ));
    out.push_str(&table(
        "context",
        &["aspect", "differs", "member_index", "value"],
        context,
    ));
    out.push_str(&table(
        "subgroups",
        &["id", "size", "members", "members_omitted", "values"],
        g.subgroups
            .subgroups
            .iter()
            .map(|s| {
                vec![
                    s.id.to_string(),
                    s.size.to_string(),
                    s.members
                        .iter()
                        .map(|m| (m + 1).to_string())
                        .collect::<Vec<_>>()
                        .join(" "),
                    s.size.saturating_sub(s.members.len()).to_string(),
                    s.values.join("; "),
                ]
            })
            .collect(),
    ));
    out
}
