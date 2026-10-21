//! `build_context_pack` with a `detail` level and an estimated `max_tokens`
//! budget. Selection and fitting live in `mct_index::select`; this module maps
//! pack parts onto candidates, renders each level and counts the *whole*
//! rendered response (header, rows, footers, budget line) with
//! [`EstimatedCounter`]. Omitting both arguments never reaches this module, so
//! the default response is the unchanged one from [`crate::format`].
//!
//! Levels for a pack:
//! - `overview`: definition locations, related symbols without signatures, no
//!   uncertain rows.
//! - `summary`: definitions reduced to their signature line, related symbols
//!   with signatures, uncertain rows with one candidate.
//! - `detailed`: the complete pack.
//!
//! `max_tokens` is an *estimated* budget (heuristic counter), not a model limit.

use std::collections::BTreeMap;

use mct_index::{
    select, Candidate, ContextBudget, DetailLevel, EstimatedCounter, Evidence, SelectedRow,
    TokenCounter,
};

use crate::format;
use crate::server::ContextPack;
use crate::toon::OutputFormat;

/// Validated `detail`/`max_tokens` arguments of a pack call.
#[derive(Debug, Clone, Copy)]
pub struct PackRequest {
    pub detail: DetailLevel,
    pub max_tokens: Option<usize>,
}

impl PackRequest {
    /// `Ok(None)` when neither argument was given: the unchanged pack.
    pub fn parse(detail: Option<&str>, max_tokens: Option<usize>) -> Result<Option<Self>, String> {
        if detail.is_none() && max_tokens.is_none() {
            return Ok(None);
        }
        let detail = match detail {
            Some(raw) => raw.parse::<DetailLevel>().map_err(|e| e.to_string())?,
            None => DetailLevel::Detailed,
        };
        // Validates `max_tokens` (zero is an error, huge values are clamped).
        let budget = ContextBudget::new(max_tokens, 1, detail).map_err(|e| e.to_string())?;
        Ok(Some(Self {
            detail,
            max_tokens: budget.max_tokens(),
        }))
    }
}

#[derive(Clone, Copy)]
enum Part {
    Definition(usize),
    Related(usize),
    Uncertain(usize),
}

fn candidates(pack: &ContextPack, limit: usize) -> (Vec<Part>, Vec<Candidate>) {
    let mut parts = Vec::new();
    let mut cands = Vec::new();
    let mut push = |part: Part, path: &str, line: u32, end: u32, ev: Evidence, score: f64| {
        cands.push(Candidate {
            // The position doubles as a unique identity, so rows that merely
            // look alike (same file and lines) are never merged here.
            symbol_id: parts_len(&parts),
            path: path.to_string(),
            start_line: line,
            end_line: end,
            evidence: ev,
            role: String::new(),
            score,
        });
        parts.push(part);
    };
    for (i, def) in pack.definitions.iter().enumerate() {
        let hit = &def.hit;
        let end = hit.end_line.unwrap_or(hit.line);
        push(
            Part::Definition(i),
            &hit.relative_path,
            hit.line,
            end,
            Evidence::Definition,
            1_000.0 - i as f64,
        );
    }
    for (j, rel) in pack.related.iter().take(limit).enumerate() {
        let (path, line, end) = rel.definition.as_ref().map_or((String::new(), 0, 0), |d| {
            (
                d.relative_path.clone(),
                d.line,
                d.end_line.unwrap_or(d.line),
            )
        });
        push(
            Part::Related(j),
            &path,
            line,
            end,
            Evidence::Relation,
            500.0 - j as f64,
        );
    }
    for (k, _) in pack.uncertain.iter().take(limit).enumerate() {
        push(
            Part::Uncertain(k),
            "",
            0,
            0,
            Evidence::Relation,
            100.0 - k as f64,
        );
    }
    (parts, cands)
}

fn parts_len(parts: &[Part]) -> i64 {
    i64::try_from(parts.len()).unwrap_or(i64::MAX)
}

/// The pack as it is rendered for the selected rows.
///
/// `caps` maps a definition (by candidate id) lowered to `summary` to the
/// number of leading snippet lines it may keep: the part of the budget the
/// level steps leave unused, spent on the head of the source.
fn view(
    pack: &ContextPack,
    parts: &[Part],
    rows: &[SelectedRow],
    caps: &BTreeMap<i64, usize>,
) -> ContextPack {
    let mut out = ContextPack {
        definitions: Vec::new(),
        related: Vec::new(),
        uncertain: Vec::new(),
        ..pack.clone()
    };
    let mut shown_definitions = 0usize;
    for row in rows {
        let Some(part) = usize::try_from(row.candidate.symbol_id)
            .ok()
            .and_then(|i| parts.get(i))
        else {
            continue;
        };
        match *part {
            Part::Definition(i) => {
                let mut def = pack.definitions[i].clone();
                match row.level {
                    DetailLevel::Detailed => {}
                    DetailLevel::Summary => match caps.get(&row.candidate.symbol_id) {
                        Some(&keep) => {
                            let total = def.snippet.lines().count();
                            def.snippet = head_lines(&def.snippet, keep);
                            def.hidden_lines +=
                                u32::try_from(total.saturating_sub(keep)).unwrap_or(u32::MAX);
                        }
                        None => {
                            def.snippet = def
                                .signature
                                .as_ref()
                                .map(|sig| format!("{}| {sig}\n", def.hit.line))
                                .unwrap_or_default();
                            def.hidden_lines = 0;
                        }
                    },
                    DetailLevel::Overview => {
                        def.snippet.clear();
                        def.hidden_lines = 0;
                    }
                }
                shown_definitions += 1;
                out.definitions.push(def);
            }
            Part::Related(j) => {
                let mut rel = pack.related[j].clone();
                if row.level == DetailLevel::Overview {
                    rel.signature = None;
                }
                out.related.push(rel);
            }
            Part::Uncertain(k) => match row.level {
                DetailLevel::Overview => {}
                level => {
                    let mut item = pack.uncertain[k].clone();
                    if level == DetailLevel::Summary {
                        item.candidates.truncate(1);
                    }
                    out.uncertain.push(item);
                }
            },
        }
    }
    out.omitted_definitions =
        pack.omitted_definitions + pack.definitions.len().saturating_sub(shown_definitions);
    out
}

fn role_counts(pack: &ContextPack) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for rel in &pack.related {
        for role in &rel.roles {
            *counts.entry(role.as_str()).or_default() += 1;
        }
    }
    if !pack.uncertain.is_empty() {
        counts.insert("uncertain", pack.uncertain.len());
    }
    counts
        .iter()
        .map(|(role, n)| format!("{role}={n}"))
        .collect::<Vec<_>>()
        .join(" ")
}

struct Stats {
    requested: DetailLevel,
    degraded: usize,
    dropped: usize,
    /// Shown as the estimate; the cost pass prints the limit here so the line
    /// is never narrower during fitting than at the end.
    estimate: usize,
    max_tokens: Option<usize>,
    /// Estimated cost of every row at the requested level (no budget).
    full: usize,
    /// Estimated cost of the whole pack at each higher level.
    higher: Vec<(DetailLevel, usize)>,
}

/// The `Budget:` line, or nothing when it would carry no information: a
/// `detailed` pack without a budget that lost no row is the plain pack.
fn budget_line(pack: &ContextPack, stats: &Stats) -> String {
    let lossy = stats.degraded > 0 || stats.dropped > 0;
    if stats.max_tokens.is_none() && stats.requested == DetailLevel::Detailed && !lossy {
        return String::new();
    }
    let mut line = format!("\nBudget: {}", if lossy { "partial" } else { "complete" });
    if let Some(max) = stats.max_tokens {
        line.push_str(&format!(" ~{}/{max}", stats.estimate));
    }
    if stats.degraded > 0 {
        line.push_str(&format!("; lowered={}", stats.degraded));
    }
    if stats.dropped > 0 {
        line.push_str(&format!("; dropped={}", stats.dropped));
    }
    if lossy {
        line.push_str(&format!("; full={}", stats.full));
    }
    if lossy || stats.requested != DetailLevel::Detailed {
        let counts = role_counts(pack);
        if !counts.is_empty() {
            line.push_str(&format!("; avail: {counts}"));
        }
    }
    if !stats.higher.is_empty() {
        let next = stats
            .higher
            .iter()
            .map(|(level, cost)| format!("{}~{cost}", level.as_str()))
            .collect::<Vec<_>>()
            .join(" ");
        line.push_str(&format!("; next: {next}"));
    }
    line.push('\n');
    line
}

fn body(
    pack: &ContextPack,
    parts: &[Part],
    rows: &[SelectedRow],
    limit: usize,
    format: OutputFormat,
    caps: &BTreeMap<i64, usize>,
) -> String {
    let shown = view(pack, parts, rows, caps);
    match format {
        OutputFormat::Text => format::context_pack(&shown, limit),
        OutputFormat::Toon => format::context_pack_toon(&shown, limit),
    }
}

fn compose(
    pack: &ContextPack,
    parts: &[Part],
    rows: &[SelectedRow],
    limit: usize,
    format: OutputFormat,
    caps: &BTreeMap<i64, usize>,
    stats: &Stats,
) -> String {
    let mut out = body(pack, parts, rows, limit, format, caps);
    out.push_str(&budget_line(pack, stats));
    out
}

/// Renders `pack` at the requested level, fitted to the estimated budget.
pub fn render(
    pack: &ContextPack,
    limit: usize,
    format: OutputFormat,
    request: &PackRequest,
) -> String {
    let (parts, cands) = candidates(pack, limit);
    let count = cands.len().max(1);
    // `request` was validated by `PackRequest::parse`, so this cannot fail;
    // an unbudgeted fallback keeps the response valid regardless.
    let Ok(budget) = ContextBudget::new(request.max_tokens, count, request.detail) else {
        return format::context_pack(pack, limit);
    };
    let total = cands.len();
    let counter = EstimatedCounter;
    // Whole pack at one level, footer excluded: what raising the level costs.
    let at_level = |level: DetailLevel| {
        let rows: Vec<SelectedRow> = cands
            .iter()
            .map(|candidate| SelectedRow {
                candidate: candidate.clone(),
                level,
                requested: request.detail,
            })
            .collect();
        counter
            .count(&body(pack, &parts, &rows, limit, format, &BTreeMap::new()))
            .value
    };
    let full = at_level(request.detail);
    let mut higher = Vec::new();
    let mut level = request.detail;
    while let Some(up) = level.higher() {
        higher.push((up, at_level(up)));
        level = up;
    }
    let placeholder = request.max_tokens.unwrap_or(0);
    let selection = select(cands.clone(), 0, &budget, |rows, _truncated| {
        let stats = Stats {
            requested: request.detail,
            degraded: rows.iter().filter(|r| r.degraded()).count(),
            dropped: total - rows.len(),
            estimate: placeholder,
            max_tokens: request.max_tokens,
            full,
            higher: higher.clone(),
        };
        counter
            .count(&compose(
                pack,
                &parts,
                rows,
                limit,
                format,
                &BTreeMap::new(),
                &stats,
            ))
            .value
    });
    let mut stats = Stats {
        requested: request.detail,
        degraded: selection.degraded,
        dropped: selection.dropped,
        estimate: selection.spent,
        max_tokens: request.max_tokens,
        full,
        higher,
    };
    // The level steps are coarse: a definition goes from its whole source to
    // its signature. Spend what the budget still has on its leading lines.
    let mut caps: BTreeMap<i64, usize> = BTreeMap::new();
    if let Some(max) = request.max_tokens.filter(|_| !selection.minimal()) {
        let placeholder = Stats {
            estimate: max,
            higher: stats.higher.clone(),
            ..stats
        };
        for row in &selection.rows {
            let index = usize::try_from(row.candidate.symbol_id).ok();
            let Some(Part::Definition(d)) = index.and_then(|i| parts.get(i)).copied() else {
                continue;
            };
            if row.level != DetailLevel::Summary || row.requested != DetailLevel::Detailed {
                continue;
            }
            let snippet = &pack.definitions[d].snippet;
            let marker = format!("{}|", pack.definitions[d].hit.line);
            let Some(floor) = snippet.lines().position(|l| l.starts_with(&marker)) else {
                continue;
            };
            let cost = |keep: usize, caps: &BTreeMap<i64, usize>| {
                let mut trial = caps.clone();
                trial.insert(row.candidate.symbol_id, keep);
                counter
                    .count(&compose(
                        pack,
                        &parts,
                        &selection.rows,
                        limit,
                        format,
                        &trial,
                        &placeholder,
                    ))
                    .value
            };
            let (mut low, mut high) = (floor + 1, snippet.lines().count());
            if high <= low || cost(low + 1, &caps) > max {
                continue;
            }
            // Largest `keep` in (low, high] that fits; `low + 1` is known to.
            low += 1;
            while low < high {
                let mid = (low + high).div_ceil(2);
                if cost(mid, &caps) <= max {
                    low = mid;
                } else {
                    high = mid - 1;
                }
            }
            caps.insert(row.candidate.symbol_id, low);
        }
        if !caps.is_empty() {
            stats.estimate = counter
                .count(&compose(
                    pack,
                    &parts,
                    &selection.rows,
                    limit,
                    format,
                    &caps,
                    &placeholder,
                ))
                .value;
        }
    }
    let mut out = compose(pack, &parts, &selection.rows, limit, format, &caps, &stats);
    if selection.minimal() {
        if let Some(needed) = selection.needed_min_tokens {
            out.push_str(&format!(
                "No row fits `max_tokens`; ~{needed} est. tokens are needed for one row.\n"
            ));
        }
    }
    out
}

/// Snippet lines a `detailed` search hit shows when `snippet_lines` is not
/// given (only reachable with an explicit `detail`/`max_tokens`).
pub const SEARCH_DETAILED_SNIPPET_LINES: usize = 8;

/// First `n` lines of a numbered snippet.
fn head_lines(snippet: &str, n: usize) -> String {
    snippet
        .lines()
        .take(n)
        .fold(String::new(), |mut acc, line| {
            acc.push_str(line);
            acc.push('\n');
            acc
        })
}

/// `search_symbols_like` at a `detail` level within an estimated budget.
/// `hits` is the whole ranked list; `snippets[i]` is the detailed snippet of
/// `hits[offset + i]` (see [`SEARCH_DETAILED_SNIPPET_LINES`]).
///
/// Levels: `overview` = location, kind, name, parent; `summary` adds the
/// definition's first line; `detailed` adds the whole snippet. The page is
/// `hits[offset..offset + limit]`; rows the budget removes are reported with
/// the `offset` that continues after the last shown row.
///
/// `prefix` and `suffix` are text the tool puts around the hits (the hybrid
/// note and its literal section). They are not rows, so they are never cut,
/// but they are counted: the budget covers the whole response.
#[allow(clippy::too_many_arguments)]
pub fn render_search(
    query: &str,
    hits: &[mct_index::SymbolHit],
    snippets: &[Option<String>],
    offset: usize,
    limit: usize,
    format: OutputFormat,
    request: &PackRequest,
    prefix: &str,
    suffix: &str,
) -> String {
    let cands: Vec<Candidate> = hits
        .iter()
        .enumerate()
        .map(|(i, hit)| Candidate {
            symbol_id: hit.id,
            path: hit.relative_path.clone(),
            start_line: hit.line,
            end_line: hit.end_line.unwrap_or(hit.line),
            evidence: Evidence::Definition,
            role: i.to_string(), // keeps identical rows from collapsing: ranks differ
            score: -(i as f64),
        })
        .collect();
    let total = hits.len();
    let Ok(budget) = ContextBudget::new(request.max_tokens, limit.max(1), request.detail) else {
        return String::new();
    };
    let rank_of = |row: &SelectedRow| row.candidate.role.parse::<usize>().unwrap_or(0);
    let compose = |rows: &[SelectedRow], next: Option<usize>, degraded: usize, spent: usize| {
        let shown: Vec<mct_index::SymbolHit> = rows
            .iter()
            .filter_map(|r| hits.get(rank_of(r)).cloned())
            .collect();
        let shown_snippets: Vec<Option<String>> = rows
            .iter()
            .map(|r| {
                let snippet = snippets.get(rank_of(r).wrapping_sub(offset))?.as_deref()?;
                match r.level {
                    DetailLevel::Overview => None,
                    DetailLevel::Summary => Some(head_lines(snippet, 1)),
                    DetailLevel::Detailed => Some(snippet.to_string()),
                }
            })
            .collect();
        let mut out = if shown.is_empty() && total > 0 {
            format!("{total} match(es) for `{query}`, none fit `max_tokens`\n")
        } else {
            let text = match format {
                OutputFormat::Text => {
                    format::search_hits(query, &shown, 0, shown.len().max(1), &shown_snippets)
                }
                OutputFormat::Toon => {
                    format::search_hits_toon(query, &shown, 0, shown.len().max(1), &shown_snippets)
                }
            };
            // The header counts the matches, as the default response does,
            // not the rows this page managed to show.
            let shown_header = format!("{} match(es) for", shown.len());
            if shown.len() == total {
                text
            } else {
                text.replacen(&shown_header, &format!("{total} match(es) for"), 1)
            }
        };
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.insert_str(0, prefix);
        if !suffix.trim().is_empty() {
            out.push_str(suffix.trim_end());
            out.push('\n');
        }
        let partial = next.is_some() || degraded > 0;
        if request.max_tokens.is_some() || partial {
            out.push_str(&format!(
                "Budget: {}",
                if partial { "partial" } else { "complete" }
            ));
            if let Some(max) = request.max_tokens {
                out.push_str(&format!(" ~{spent}/{max}"));
            }
            if partial {
                out.push_str(&format!(", {} of {total} seen", offset + rows.len()));
            }
            if degraded > 0 {
                out.push_str(&format!("; lowered={degraded}"));
            }
            if let Some(next) = next {
                out.push_str(&format!("; next: offset={next}"));
            }
            out.push('\n');
        }
        out
    };
    let counter = EstimatedCounter;
    let max = request.max_tokens.unwrap_or(0);
    let selection = select(cands, offset, &budget, |rows, truncated| {
        let degraded = rows.iter().filter(|r| r.degraded()).count();
        let next = truncated.then_some(offset + rows.len());
        counter.count(&compose(rows, next, degraded, max)).value
    });
    let mut out = compose(
        &selection.rows,
        selection.next_offset,
        selection.degraded,
        selection.spent,
    );
    if selection.minimal() {
        if let Some(needed) = selection.needed_min_tokens {
            out.push_str(&format!(
                "No row fits `max_tokens`; ~{needed} est. tokens are needed for one row.\n"
            ));
        }
    }
    out
}
