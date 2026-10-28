//! Header status, dashboard summary and measured statistics.

use std::collections::BTreeMap;
use std::time::UNIX_EPOCH;

use serde_json::{json, Value};

use mct_index::reviews::{Resolution, State};
use mct_index::UnsupportedFile;

use crate::web::Site;

const DAY: u64 = 86_400;
const RECENT_EVENTS: usize = 8;

/// One row per file the indexer reported: where it is, the line the parser stopped at and the
/// innermost indexed symbol that line falls in. `detail` is `line N: message` for a syntax error.
fn issue_rows(site: &Site, issues: &[UnsupportedFile]) -> Vec<Value> {
    let mut rows: Vec<Value> = issues
        .iter()
        .map(|issue| {
            let (line, message) = issue
                .detail
                .strip_prefix("line ")
                .and_then(|rest| rest.split_once(": "))
                .and_then(|(number, message)| Some((number.parse::<u32>().ok()?, message)))
                .map_or((None, issue.detail.as_str()), |(line, message)| (Some(line), message));
            let path = issue.relative_path.as_str();
            let language = site
                .navigation
                .files
                .binary_search_by(|file| file.relative_path.as_str().cmp(path))
                .ok()
                .map(|index| site.navigation.files[index].language.as_str());
            let symbol = line.and_then(|line| {
                site.symbols
                    .get(path)?
                    .iter()
                    .map(|id| &site.navigation.symbols[*id])
                    .filter(|symbol| symbol.line <= line && line <= symbol.end_line.unwrap_or(symbol.line))
                    .min_by_key(|symbol| symbol.end_line.unwrap_or(symbol.line) - symbol.line)
                    .map(|symbol| json!({ "name": symbol.name, "kind": symbol.kind, "line": symbol.line, "endLine": symbol.end_line }))
            });
            json!({ "path": path, "name": path.rsplit('/').next().unwrap_or(path), "line": line,
                    "message": message, "language": language, "symbol": symbol })
        })
        .collect();
    rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    rows
}

/// Number of indexed files modified after the last index run (bounded stat calls).
fn stale_files(site: &Site, indexed_at: Option<i64>) -> Option<usize> {
    let indexed_at = u64::try_from(indexed_at?).ok()?;
    Some(
        site.navigation
            .files
            .iter()
            .filter(|file| {
                std::fs::metadata(site.root.join(&file.relative_path))
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .is_some_and(|time| time.as_secs() > indexed_at)
            })
            .count(),
    )
}

pub(super) fn meta(site: &Site) -> Value {
    let status = site.index.status().ok();
    let indexed_at = status.as_ref().and_then(|status| status.last_indexed_at);
    let stale = stale_files(site, indexed_at);
    // The web server is read-only and never runs the indexer, so the only
    // states it can observe are: no record, failed files, stale files, ready.
    let state = match (&status, indexed_at, stale) {
        (None, ..) | (_, None, _) => "unknown",
        (Some(status), ..) if !status.read_failures.is_empty() => "failed",
        (_, _, Some(count)) if count > 0 => "stale",
        _ => "ready",
    };
    json!({
        "project": site.project,
        "generated": site.generated,
        "csrf": site.csrf,
        "warning": site.warning,
        // Set when a review file cannot be read: nothing is hidden and
        // review actions are disabled until a person fixes the file.
        "reviewsReadOnly": site.reviews.read_only_reason(),
        "reviewsFile": format!(".mct-index/{}", mct_index::reviews::CANONICAL_FILE),
        "indexing": {
            "state": state,
            "lastIndexedAt": indexed_at,
            "staleFiles": stale,
            "syntaxErrors": status.as_ref().map(|s| s.syntax_errors.len()),
            "readFailures": status.as_ref().map(|s| s.read_failures.len()),
            "syntaxIssues": status.as_ref().map(|s| issue_rows(site, &s.syntax_errors)),
            "readIssues": status.as_ref().map(|s| issue_rows(site, &s.read_failures)),
        },
        "vault": { "notes": site.vault.notes.len(), "truncated": site.vault.truncated },
        // What the backend can really do. `false`/`null` entries are shown as
        // unavailable by the UI instead of being faked.
        "capabilities": {
            "duplicates": true,
            "deadCode": true,
            "reviews": true,
            "dependencies": { "imports": true, "calls": false, "cycles": true },
            "search": { "keyword": true, "literals": true, "semantic": false },
            "obsidian": { "read": true, "edit": false, "openInObsidian": false },
            "history": false,
            "reindex": false,
            "tokens": false,
            "similarityScore": false,
            "confidence": false,
            "severity": false,
            "reviewers": false,
        },
    })
}

fn finding_label(site: &Site, key: &str) -> Value {
    if let Some(id) = site
        .review_keys
        .iter()
        .position(|candidate| candidate == key)
    {
        let group = &site.duplicates.groups[id];
        let title = group
            .members
            .iter()
            .take(2)
            .map(|m| m.symbol_name.as_str())
            .collect::<Vec<_>>()
            .join(" ≈ ");
        return json!({ "key": key, "type": "duplicate", "title": title, "path": group.members[0].path });
    }
    site.dead
        .iter()
        .find(|candidate| candidate.key == key)
        .map(|candidate| {
            let symbol = &site.navigation.symbols[candidate.symbol];
            json!({ "key": key, "type": "dead-code", "title": symbol.name, "path": symbol.relative_path })
        })
        .unwrap_or(Value::Null)
}

pub(super) fn summary(site: &Site) -> Value {
    let status = site.index.status().ok();
    let mut languages: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for file in &site.navigation.files {
        languages.entry(&file.language).or_default().0 += 1;
    }
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for symbol in &site.navigation.symbols {
        languages.entry(&symbol.language).or_default().1 += 1;
        *kinds.entry(&symbol.kind).or_default() += 1;
    }
    let mut by_language: Vec<_> = languages.into_iter().collect();
    by_language.sort_by(|a, b| b.1 .1.cmp(&a.1 .1).then(a.0.cmp(b.0)));
    let mut by_kind: Vec<_> = kinds.into_iter().collect();
    by_kind.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    let total_findings = site.duplicates.groups.len() + site.dead.len();
    let mut states: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut by_type: BTreeMap<&'static str, BTreeMap<&'static str, usize>> = BTreeMap::new();
    let all_keys = site
        .review_keys
        .iter()
        .map(|key| ("duplicate", key))
        .chain(site.dead.iter().map(|d| ("dead-code", &d.key)));
    for (kind, key) in all_keys {
        let state = site
            .reviews
            .resolution(key)
            .map_or("unreviewed", Resolution::effective_label);
        *states.entry(state).or_default() += 1;
        *by_type.entry(kind).or_default().entry(state).or_default() += 1;
    }

    // Review decisions per day, from the audit history (the only history
    // that exists). Events without a time (version 1 files) have no day.
    let mut per_day: BTreeMap<u64, BTreeMap<&'static str, usize>> = BTreeMap::new();
    let mut events: Vec<_> = site.reviews.events().collect();
    events.sort_by_key(|(_, event)| std::cmp::Reverse(event.at));
    for (_, event) in events.iter().filter(|(_, event)| event.at > 0) {
        let to = match event.to {
            State::Unreviewed => "reopened",
            state => state.value(),
        };
        *per_day
            .entry(event.at / 1000 / DAY)
            .or_default()
            .entry(to)
            .or_default() += 1;
    }
    let recent: Vec<Value> = events
        .iter()
        .take(RECENT_EVENTS)
        .map(|(key, event)| {
            json!({
                "at": (event.at > 0).then_some(event.at / 1000),
                "to": event.to.value(),
                "note": event.note,
                "finding": finding_label(site, key),
            })
        })
        .collect();

    json!({
        "files": site.navigation.files.len(),
        "symbols": site.navigation.symbols.len(),
        "languages": by_language.iter().map(|(l, (f, s))| json!({ "language": l, "files": f, "symbols": s })).collect::<Vec<_>>(),
        "kinds": by_kind.iter().map(|(k, n)| json!({ "kind": k, "count": n })).collect::<Vec<_>>(),
        "duplicates": { "groups": site.duplicates.groups.len(), "total": site.duplicates.total_groups,
                        "complete": site.duplicates.complete, "skipped": site.duplicates.skipped.len() },
        "deadCode": { "candidates": site.dead.len() },
        "findings": { "total": total_findings, "states": states, "byType": by_type },
        "notes": site.vault.notes.len(),
        "reviewActivity": per_day.iter().map(|(day, counts)| json!({ "day": day * DAY, "counts": counts })).collect::<Vec<_>>(),
        "recent": recent,
        "health": {
            "lastIndexedAt": status.as_ref().and_then(|s| s.last_indexed_at),
            "syntaxErrors": status.as_ref().map(|s| s.syntax_errors.len()),
            "readFailures": status.as_ref().map(|s| s.read_failures.len()),
            "duplicateAnalysisComplete": site.duplicates.complete,
            "pendingReviews": states.get("unreviewed").copied().unwrap_or(0) + states.get("in-review").copied().unwrap_or(0),
        },
        // No indexing job history is stored, so no last-run duration exists.
        "lastIndexDurationMs": null,
        "history": null,
    })
}

fn percentile(sorted: &[u64], fraction: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() as f64 - 1.0) * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

pub(super) fn stats(site: &Site) -> Value {
    let latency: Vec<Value> = site
        .latency
        .borrow()
        .iter()
        .map(|(route, samples)| {
            let mut sorted = samples.clone();
            sorted.sort_unstable();
            json!({ "route": route, "samples": sorted.len(), "p50Us": percentile(&sorted, 0.5),
                    "p95Us": percentile(&sorted, 0.95), "maxUs": sorted.last().copied().unwrap_or(0) })
        })
        .collect();
    let size = |path: std::path::PathBuf| std::fs::metadata(path).ok().map(|meta| meta.len());
    let mut value = summary(site);
    value["measurements"] = json!({
        "snapshotLoadMs": site.timings.load_ms,
        "duplicateAnalysisMs": site.timings.duplicates_ms,
        "deadCodeAnalysisMs": site.timings.dead_code_ms,
        "indexDatabaseBytes": size(site.root.join(".mct-index/index.sqlite3")),
        "apiLatency": latency,
        // Not measured by this server: indexing runs, search latency across
        // sessions, MCP response size, tokens, CPU and memory.
        "tokens": null, "mcpResponseBytes": null, "cpu": null, "memory": null,
        "incrementalIndexMs": null,
    });
    value
}
