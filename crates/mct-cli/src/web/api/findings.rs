//! Findings: duplicate groups and dead-code candidates with their review state.
//!
//! One list endpoint serves the Duplicates, Dead code and Reviews pages; they
//! differ only in the `type` filter, so filtering and paging exist once.

use serde_json::{json, Value};

use mct_index::reviews::{HistoryEvent, Resolution};

use super::{code, contains_ci, csv, facet, page_bounds, text, Params};
use crate::web::Site;

const MEMBER_PREVIEW: usize = 6;
const UNREVIEWED: &str = "unreviewed";

/// Review times are milliseconds; the API, like the rest of it, uses Unix
/// seconds. A time of 0 means the source recorded none (`A1`).
fn known_time(at: u64) -> Option<u64> {
    (at > 0).then_some(at / 1000)
}

fn event_json(event: &HistoryEvent) -> Value {
    json!({
        "from": event.from.value(),
        "to": event.to.value(),
        "at": known_time(event.at),
        "note": event.note,
        "source": event.format.value(),
    })
}

/// `state` is what the page shows: a stored state, `needs-reevaluation` or
/// `resolved-but-detected`; `previousState` is the stored one behind it.
fn review_fields(site: &Site, key: &str) -> Value {
    let resolution = site.reviews.resolution(key);
    let winner = resolution.and_then(|resolution| resolution.winner.as_ref());
    json!({
        "key": key,
        "state": resolution.map_or(UNREVIEWED, Resolution::label),
        "previousState": resolution.map_or(UNREVIEWED, |resolution| resolution.state().value()),
        "note": winner.map_or("", |winner| winner.note.as_str()),
        "updated": winner.and_then(|winner| known_time(winner.at)),
        "source": winner.map(|winner| winner.format.value()),
        "revision": resolution.map_or_else(|| "none".to_string(), Resolution::revision),
        "readOnly": site.reviews.read_only_reason().is_some(),
    })
}

fn history_json(site: &Site, key: &str) -> Value {
    site.reviews.history(key).iter().map(event_json).collect()
}

pub(in crate::web) fn review_json(site: &Site, key: &str) -> Value {
    let mut value = review_fields(site, key);
    value["history"] = history_json(site, key);
    value
}

fn top_directory(path: &str) -> &str {
    path.split('/').next().unwrap_or(path)
}

pub(super) fn duplicate_row(site: &Site, id: usize) -> Value {
    let group = &site.duplicates.groups[id];
    let key = &site.review_keys[id];
    let paths: std::collections::BTreeSet<&str> =
        group.members.iter().map(|m| m.path.as_str()).collect();
    let modules: std::collections::BTreeSet<&str> =
        paths.iter().map(|p| top_directory(p)).collect();
    let scope = if paths.len() == 1 {
        "same-file"
    } else if modules.len() == 1 {
        "cross-file"
    } else {
        "cross-module"
    };
    let members: Vec<Value> = group
        .members
        .iter()
        .take(MEMBER_PREVIEW)
        .map(|m| json!({ "path": m.path, "start": m.start_line, "end": m.end_line, "name": m.symbol_name, "kind": m.symbol_kind }))
        .collect();
    let first = &group.members[0];
    let mut row = review_fields(site, key);
    row["type"] = json!("duplicate");
    row["rank"] = json!(id);
    row["group"] = json!(id);
    row["title"] = json!(group
        .members
        .iter()
        .take(2)
        .map(|m| m.symbol_name.as_str())
        .collect::<Vec<_>>()
        .join(" ≈ "));
    row["language"] = json!(group.language);
    row["path"] = json!(first.path);
    row["line"] = json!(first.start_line);
    row["kind"] = json!(first.symbol_kind);
    row["lines"] = json!(group.line_count);
    row["tokens"] = json!(group.token_count);
    row["verdict"] = json!(group
        .verdict
        .level
        .as_str()
        .to_lowercase()
        .replace('_', "-"));
    row["priority"] = json!(group.priority.level.as_str());
    row["scope"] = json!(scope);
    row["memberCount"] = json!(group.members.len());
    row["members"] = Value::Array(members);
    row
}

pub(super) fn dead_row(site: &Site, index: usize) -> Value {
    let candidate = &site.dead[index];
    let symbol = &site.navigation.symbols[candidate.symbol];
    let mut row = review_fields(site, &candidate.key);
    row["type"] = json!("dead-code");
    row["rank"] = json!(index);
    row["symbolId"] = json!(candidate.symbol);
    row["title"] = json!(symbol.name);
    row["language"] = json!(symbol.language);
    row["path"] = json!(symbol.relative_path);
    row["line"] = json!(symbol.line);
    row["endLine"] = json!(symbol.end_line);
    row["kind"] = json!(symbol.kind);
    row["parent"] = json!(symbol.parent);
    row["refs"] = json!(0);
    // The heuristic has no confidence or severity model: say so, do not guess.
    row["confidence"] = json!("unknown");
    row["severity"] = Value::Null;
    row
}

fn haystack(row: &Value) -> String {
    let mut text = format!(
        "{} {}",
        row["title"].as_str().unwrap_or(""),
        row["path"].as_str().unwrap_or("")
    );
    for member in row["members"].as_array().into_iter().flatten() {
        text.push(' ');
        text.push_str(member["name"].as_str().unwrap_or(""));
        text.push(' ');
        text.push_str(member["path"].as_str().unwrap_or(""));
    }
    text
}

fn sort_key(row: &Value, field: &str) -> (i64, String) {
    match &row[field] {
        Value::Number(number) => (number.as_i64().unwrap_or(0), String::new()),
        Value::String(text) => (0, text.to_lowercase()),
        _ => (0, String::new()),
    }
}

const FILTER_FIELDS: [&str; 7] = [
    "type",
    "state",
    "language",
    "kind",
    "verdict",
    "scope",
    "confidence",
];

pub(super) fn list(site: &Site, params: &Params) -> Value {
    let wanted = text(params, "type");
    let mut rows: Vec<Value> = Vec::new();
    if wanted != "dead-code" {
        rows.extend((0..site.duplicates.groups.len()).map(|id| duplicate_row(site, id)));
    }
    if wanted != "duplicate" {
        rows.extend((0..site.dead.len()).map(|index| dead_row(site, index)));
    }
    let query = text(params, "q");
    let dir = text(params, "dir").trim_end_matches('/');
    rows.retain(|row| {
        let path = row["path"].as_str().unwrap_or("");
        (dir.is_empty()
            || path == dir
            || path
                .strip_prefix(dir)
                .is_some_and(|rest| rest.starts_with('/')))
            && contains_ci(&haystack(row), query)
    });
    let facets: serde_json::Map<String, Value> = FILTER_FIELDS
        .iter()
        .map(|field| {
            (
                (*field).to_string(),
                facet(rows.iter().filter_map(|row| row[*field].as_str())),
            )
        })
        .collect();
    for field in FILTER_FIELDS {
        let allowed = csv(params, field);
        if !allowed.is_empty() {
            rows.retain(|row| allowed.contains(&row[field].as_str().unwrap_or("")));
        }
    }
    let sort = match text(params, "sort") {
        "" => "rank",
        other => other,
    };
    rows.sort_by(|a, b| {
        sort_key(a, sort)
            .cmp(&sort_key(b, sort))
            .then(sort_key(a, "rank").cmp(&sort_key(b, "rank")))
    });
    if text(params, "order") == "desc" {
        rows.reverse();
    }
    let (page, size, start) = page_bounds(params, rows.len(), 25);
    let total = rows.len();
    let items: Vec<Value> = rows.into_iter().skip(start).take(size).collect();
    json!({ "total": total, "page": page, "size": size, "items": items, "facets": facets })
}

/// Lines of member code sent with one duplicate group; members past it fall back to `excerpt`.
const MEMBER_CODE_BUDGET: usize = 1500;

pub(super) fn detail(site: &Site, params: &Params) -> Result<Value, (u16, &'static str)> {
    let key = text(params, "key");
    if let Some(id) = site
        .review_keys
        .iter()
        .position(|candidate| candidate == key)
    {
        let group = &site.duplicates.groups[id];
        let mut row = duplicate_row(site, id);
        let mut budget = MEMBER_CODE_BUDGET;
        row["members"] = group
            .members
            .iter()
            .map(|m| {
                // The full body, so an assistant can compare the copies; `excerpt` is only the start.
                let body = code::read_lines(site, &m.path, m.start_line as usize, m.end_line as usize)
                    .filter(|(lines, _)| lines.len() <= budget);
                budget -= body.as_ref().map_or(0, |(lines, _)| lines.len());
                json!({ "path": m.path, "start": m.start_line, "end": m.end_line, "name": m.symbol_name,
                        "kind": m.symbol_kind, "lines": m.line_count, "tokens": m.token_count, "excerpt": m.excerpt,
                        "code": body.as_ref().map(|(lines, _)| lines), "codeTruncated": body.as_ref().is_some_and(|(_, cut)| *cut) })
            })
            .collect();
        row["reasons"] = group
            .verdict
            .reasons
            .iter()
            .map(|reason| reason.as_str())
            .collect();
        row["priorityReason"] = json!(group.priority.reason);
        row["method"] = json!({
            "kind": "token-sequence",
            "normalization": site.duplicates.options.normalization.as_str(),
        });
        let mut analysis = String::new();
        crate::duplicates::render_group(
            &mut analysis,
            id + 1,
            group,
            site.duplicates.options.normalization,
        );
        row["analysis"] = json!(analysis);
        row["history"] = history_json(site, key);
        return Ok(row);
    }
    let index = site
        .dead
        .iter()
        .position(|candidate| candidate.key == key)
        .ok_or((404, "unknown finding"))?;
    let mut row = dead_row(site, index);
    let symbol_id = site.dead[index].symbol;
    let mut params = Params::new();
    params.insert("id".into(), symbol_id.to_string());
    row["detail"] = code::symbol(site, &params)?;
    row["method"] = json!({ "kind": "no-indexed-reference-by-name" });
    row["history"] = history_json(site, key);
    Ok(row)
}
