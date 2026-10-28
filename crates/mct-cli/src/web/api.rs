//! Read-only JSON API under `/<token>/api/` (ADR-019 D6). GET only; every
//! response is built from the immutable snapshot plus the dashboard store.
//! JSON is the browser's native `fetch` format; agent-facing exports stay TOON.

use serde_json::{json, Value};

use super::render::{Req, Response};
use super::reviews::{self, Kind, Record, State};
use super::stats::Bucket;
use super::{index_stamp, member_reviews, now, source, store, view_dead, view_obsidian, Site};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

/// Refresh analysis-bearing reads, including rendered pages. Assets and version
/// polling do not trigger analysis. Reuse `Site::refresh_if_stale`; POSTs retain
/// their stale-snapshot guard instead of silently rebasing writes.
pub(super) fn wants_fresh_snapshot(route: &str) -> bool {
    (route.starts_with("api/") && route != "api/version")
        || matches!(
            route,
            "duplicados" | "duplicado" | "muerto" | "simbolo" | "revisiones" | "estado"
        )
}

pub(super) fn route(req: &Req<'_>, route: &str) -> Response {
    let body = match route {
        "version" => version(req.site),
        "summary" => summary(req.site),
        "duplicates" => return duplicates(req),
        "dead" => return dead(req),
        "reviews" => return reviews(req),
        "finding" => return finding(req),
        "notes" => return notes(req),
        "note" => return note(req),
        "graph" => return graph(req),
        _ => return Response::error(404),
    };
    json_response(&body)
}

fn json_response(value: &Value) -> Response {
    Response::text("application/json; charset=utf-8", value.to_string())
}

fn nanos(time: Option<std::time::SystemTime>) -> u128 {
    time.and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_nanos())
}

/// A cheap change token: two `stat` calls, one pragma and a counter. The
/// browser refetches the visible view when `token` differs from the last one.
fn version(site: &Site) -> Value {
    let current = nanos(index_stamp(&site.root));
    let (external, own) = site.reviews.change_token().unwrap_or((0, 0));
    json!({
        "token": format!("{current}.{}.{}.{external}.{own}", nanos(site.index_stamp), site.generated),
        "index_stamp": current.to_string(),
        "snapshot_at": site.generated,
        "stale": current != nanos(site.index_stamp),
        "rebuilding": site.rebuilding.is_some(),
        "rebuild_ms": site.rebuild_ms,
    })
}

/// Pins both analysis and user-state generations across paged reads.
fn snapshot(site: &Site) -> anyhow::Result<String> {
    let (external, own) = site.reviews.change_token()?;
    Ok(format!(
        "{}.{}.{external}.{own}",
        nanos(site.index_stamp),
        site.generated
    ))
}

/// What a pinned read must not see change underneath it.
#[derive(Clone, Copy)]
enum Pin {
    /// Paged lists: the analysis and every review write (status filters and
    /// counts depend on both).
    Everything,
    /// Source chunks: only the analysis. A review saved meanwhile cannot
    /// change indexed source, so it must not invalidate the code being read.
    Analysis,
}

/// The analysis part of a snapshot token, `<index stamp>.<generated>`.
fn analysis_generation(token: &str) -> &str {
    token.rsplitn(3, '.').nth(2).unwrap_or(token)
}

fn same_generation(pin: Pin, wanted: &str, current: &str) -> bool {
    match pin {
        Pin::Everything => wanted == current,
        Pin::Analysis => analysis_generation(wanted) == analysis_generation(current),
    }
}

fn pinned_snapshot(req: &Req<'_>, pin: Pin) -> Result<String, Response> {
    let token = snapshot(req.site).map_err(|_| Response::error(500))?;
    if req
        .params
        .get("snapshot")
        .filter(|wanted| !wanted.is_empty())
        .is_some_and(|wanted| !same_generation(pin, wanted, &token))
    {
        return Err(Response::error(409));
    }
    Ok(token)
}

fn stable_response(site: &Site, token: &str, pin: Pin, value: Value) -> Response {
    match snapshot(site) {
        Ok(current) if same_generation(pin, token, &current) => json_response(&value),
        Ok(_) => Response::error(409),
        Err(_) => Response::error(500),
    }
}

fn state_name(state: &State) -> &'static str {
    match state {
        State::Pending => "pending",
        State::Decided { status, .. } => status.value(),
        State::NeedsReevaluation { .. } => "needs_reevaluation",
    }
}

fn valid_filter(status: &str) -> bool {
    matches!(
        status,
        "" | "all" | "pending" | "confirmed" | "false_positive" | "ignored" | "needs_reevaluation"
    )
}

fn matches_status(state: &State, status: &str) -> bool {
    matches!(status, "" | "all") || state_name(state) == status
}

fn state_counts(states: impl Iterator<Item = State>) -> Value {
    let mut counts = json!({"all":0,"pending":0,"confirmed":0,"false_positive":0,"ignored":0,"needs_reevaluation":0});
    for state in states {
        for key in ["all", state_name(&state)] {
            counts[key] = json!(counts[key].as_u64().unwrap_or(0) + 1);
        }
    }
    counts
}

fn state_json(state: &State) -> Value {
    match state {
        State::Pending => json!({"status": "pending"}),
        State::Decided { status, at, reason } => {
            json!({"status": status.value(), "at": at, "reason": reason})
        }
        State::NeedsReevaluation { status, at, reason } => json!({
            "status": "needs_reevaluation", "previous": status.value(),
            "at": at, "reason": reason,
        }),
    }
}

fn record_json(record: &Record) -> Value {
    json!({
        "kind": record.kind.value(), "identity": record.identity,
        "fingerprint": record.fingerprint, "status": record.status.value(),
        "at": record.at, "reason": record.reason,
    })
}

fn summary(site: &Site) -> Value {
    let counts = site.counts();
    let languages: Vec<Value> = site
        .stats
        .languages
        .iter()
        .map(|(name, language)| {
            json!({"language": name, "files": language.files, "symbols": language.symbols.iter().sum::<usize>()})
        })
        .collect();
    let buckets: serde_json::Map<String, Value> = Bucket::ALL
        .iter()
        .map(|bucket| (bucket.value().to_string(), json!(site.stats.count(*bucket))))
        .collect();
    json!({
        "project": site.project,
        "snapshot_at": site.generated,
        "files": site.stats.files,
        "directories": site.stats.directories,
        "symbols": site.stats.total_symbols(),
        "imports": site.stats.imports,
        "buckets": buckets,
        "notes": site.notes.list.len(),
        "note_links": site.notes.link_count,
        "languages": languages,
        "duplicate_groups": site.duplicate_keys.len(),
        "dead_candidates": site.dead.len(),
        "duplicates_pending": counts.duplicates_pending,
        "dead_pending": counts.dead_pending,
        "needs_reevaluation": counts.reevaluate,
        "duplicate_counts": state_counts((0..site.duplicate_keys.len()).map(|id| site.duplicate_state(id))),
        "dead_counts": state_counts((0..site.dead.len()).map(|id| site.dead_state(id))),
        "orphan_findings": site.reviews.stored_findings().map(|rows| rows.iter().filter(|r| site.fingerprint_of(r.kind, &r.identity).is_none()).count()).unwrap_or(0),
        "warning": site.warning,
    })
}

/// `offset` and `limit` must be plain numbers; `limit` is clamped.
fn page(req: &Req<'_>) -> Result<(usize, usize), Response> {
    let number = |key: &str, default: usize| match req.params.get(key) {
        None => Ok(default),
        Some(value) => value.parse::<usize>().map_err(|_| Response::error(400)),
    };
    Ok((
        number("offset", 0)?,
        number("limit", DEFAULT_LIMIT)?.clamp(1, MAX_LIMIT),
    ))
}

/// `source_offset` (default 0) and `source_limit` (default and maximum 400)
/// of a full-source request; both must be plain numbers.
fn source_page(req: &Req<'_>) -> Result<(u32, u32), Response> {
    let number = |key: &str, default: u32| match req.params.get(key) {
        None => Ok(default),
        Some(value) => value.parse::<u32>().map_err(|_| Response::error(400)),
    };
    Ok((
        number("source_offset", 0)?,
        number("source_limit", 400)?.clamp(1, 400),
    ))
}

fn duplicates(req: &Req<'_>) -> Response {
    let site = req.site;
    let only = req.params.get("id").map(String::as_str);
    let full = only.is_some() && req.param("full") == "1";
    let pin = if full { Pin::Analysis } else { Pin::Everything };
    let token = match pinned_snapshot(req, pin) {
        Ok(token) => token,
        Err(response) => return response,
    };
    if !valid_filter(req.param("status")) {
        return Response::error(400);
    }
    let (offset, limit) = match page(req) {
        Ok(page) => page,
        Err(response) => return response,
    };
    let (source_offset, source_limit) = match source_page(req) {
        Ok(page) => page,
        Err(response) => return response,
    };
    let query = req.param("q").to_lowercase();
    let candidates: Vec<_> = site
        .duplicates
        .groups
        .iter()
        .enumerate()
        .filter(|(id, group)| {
            only.is_none_or(|wanted| site.duplicate_keys[*id].identity == wanted)
                && (query.is_empty()
                    || group.language.to_lowercase().contains(&query)
                    || group.members.iter().any(|m| {
                        m.path.to_lowercase().contains(&query)
                            || m.symbol_name.to_lowercase().contains(&query)
                    }))
        })
        .map(|(id, group)| (id, group, site.duplicate_state(id)))
        .collect();
    let counts = state_counts(candidates.iter().map(|(_, _, state)| state.clone()));
    let filtered: Vec<_> = candidates
        .iter()
        .filter(|(_, _, state)| matches_status(state, req.param("status")))
        .collect();
    let items: anyhow::Result<Vec<Value>> = filtered.iter().skip(offset).take(limit).map(|(id, group, state)| {
        let key = &site.duplicate_keys[*id];
        let saved = site.reviews.file_reviews(&key.identity)?;
        let copies = member_reviews::resolve(&key.members, &saved);
        let members: Vec<Value> = group.members.iter().zip(&copies).map(|(member, copy)| {
            let total = member.end_line.saturating_sub(member.start_line).saturating_add(1);
            let requested = member.start_line.checked_add(source_offset).filter(|_| source_offset < total);
            let hash = site.navigation.files.iter().find(|f| f.relative_path == member.path).map(|f| f.content_hash.as_str());
            let excerpt = if full {
                requested.zip(hash).and_then(|(first, hash)| {
                    source::indexed_excerpt(&site.root, &member.path, hash, first,
                        first.saturating_add(source_limit - 1).min(member.end_line))
                })
            } else { None };
            let available = excerpt.is_some() || (full && source_offset >= total);
            let returned = excerpt.as_ref().map_or(0, |found| found.lines.len() as u32);
            let next = source_offset.saturating_add(returned);
            let next_offset = (full && available && next < total && returned > 0).then_some(next);
            let empty = Vec::new();
            let excerpt_lines = if full { excerpt.as_ref().map_or(&empty, |found| &found.lines) } else { &member.excerpt };
            // The editor shows each copy in its file: the lines above it on the
            // first chunk, the lines below it on the last one.
            let around = if full && excerpt.is_some() {
                hash.and_then(|hash| source::indexed_surroundings(&site.root, &member.path, hash,
                    (member.start_line, member.end_line), source::DUPLICATE_CONTEXT))
            } else { None };
            let context_before = around.as_ref().filter(|_| source_offset == 0).map_or(&empty, |found| &found.before);
            let context_after = around.as_ref().filter(|_| next_offset.is_none()).map_or(&empty, |found| &found.after);
            json!({
                "path": member.path, "member_key": copy.key, "start_line": member.start_line, "end_line": member.end_line,
                "symbol": member.symbol_name, "kind": member.symbol_kind,
                "line_count": member.line_count, "token_count": member.token_count,
                "excerpt": excerpt_lines,
                "excerpt_first": requested.unwrap_or(member.start_line),
                "excerpt_available": !full || available,
                "excerpt_truncated": if full { source_offset > 0 || next_offset.is_some() || (!available && source_offset < total) } else { true },
                "source_offset": source_offset, "source_next_offset": next_offset, "source_total": total,
                "context_before": context_before, "context_after": context_after,
                "file_lines": around.as_ref().map(|found| found.file_lines),
            })
        }).collect();
        Ok(json!({"identity":key.identity,"fingerprint":key.fingerprint,
            "revision":site.reviews.decision_revision(Kind::Duplicate,&key.identity).unwrap_or(0),
            "language":group.language,"line_count":group.line_count,"token_count":group.token_count,
            "members":members,"state":state_json(state)}))
    }).collect();
    let items = match items {
        Ok(items) => items,
        Err(error) => {
            eprintln!("warning: cannot read copy labels: {error:#}");
            return Response::error(500);
        }
    };
    stable_response(
        site,
        &token,
        pin,
        json!({"total":filtered.len(),"offset":offset,"limit":limit,"snapshot":token,"counts":counts,"items":items}),
    )
}

/// Dead-code candidates. With `id` and `full=1`, the one candidate also carries
/// a page of its source plus a few context lines, verified against the indexed
/// revision like a duplicate's full source.
fn dead(req: &Req<'_>) -> Response {
    let site = req.site;
    let only = req.params.get("id").map(String::as_str);
    let full = only.is_some() && req.param("full") == "1";
    let pin = if full { Pin::Analysis } else { Pin::Everything };
    let token = match pinned_snapshot(req, pin) {
        Ok(token) => token,
        Err(response) => return response,
    };
    if !valid_filter(req.param("status")) {
        return Response::error(400);
    }
    let (offset, limit) = match page(req) {
        Ok(page) => page,
        Err(response) => return response,
    };
    let (source_offset, source_limit) = match source_page(req) {
        Ok(page) => page,
        Err(response) => return response,
    };
    let query = req.param("q").to_lowercase();
    let candidates: Vec<_> = site
        .dead
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            only.is_none_or(|wanted| d.identity == wanted)
                && (query.is_empty()
                    || [
                        &d.entry.name,
                        &d.entry.relative_path,
                        &d.entry.kind,
                        &d.entry.language,
                    ]
                    .iter()
                    .any(|v| v.to_lowercase().contains(&query))
                    || d.entry
                        .parent
                        .as_ref()
                        .is_some_and(|p| p.to_lowercase().contains(&query)))
        })
        .map(|(id, d)| (d, site.dead_state(id)))
        .collect();
    let counts = state_counts(candidates.iter().map(|(_, state)| state.clone()));
    let filtered: Vec<_> = candidates
        .iter()
        .filter(|(_, state)| matches_status(state, req.param("status")))
        .collect();
    let items:Vec<Value> = filtered.iter().skip(offset).take(limit).map(|(d,state)| {
        let entry = &d.entry;
        let end = entry.end_line.unwrap_or(entry.line).max(entry.line);
        let mut item = json!({
            "identity":d.identity,"fingerprint":d.fingerprint,"revision":site.reviews.decision_revision(Kind::Dead,&d.identity).unwrap_or(0),
            "name":entry.name,"kind":entry.kind,"language":entry.language,"path":entry.relative_path,
            "line":entry.line,"end_line":entry.end_line,"parent":entry.parent,"state":state_json(state),
            "line_count":end - entry.line + 1,
            "cautions":view_dead::caution_codes(&entry.language,&entry.relative_path,&entry.kind),
        });
        if full {
            let window = site.navigation.files.iter().find(|f| f.relative_path == entry.relative_path)
                .and_then(|file| source::indexed_window(&site.root, &entry.relative_path, &file.content_hash,
                    (entry.line, end), (source_offset, source_limit)));
            let total = window.as_ref().map_or(0, |w| w.last - w.first + 1);
            let returned = window.as_ref().map_or(0, |w| w.lines.len() as u32);
            let next = source_offset.saturating_add(returned);
            let next_offset = (returned > 0 && next < total).then_some(next);
            if let Value::Object(map) = &mut item {
                map.extend([
                    ("excerpt".to_string(), json!(window.as_ref().map_or(&Vec::new(), |w| &w.lines))),
                    ("excerpt_first".to_string(), json!(window.as_ref().map_or(entry.line, |w| w.page_first))),
                    ("window_first".to_string(), json!(window.as_ref().map(|w| w.first))),
                    ("window_last".to_string(), json!(window.as_ref().map(|w| w.last))),
                    ("excerpt_available".to_string(), json!(window.is_some())),
                    ("excerpt_truncated".to_string(), json!(source_offset > 0 || next_offset.is_some())),
                    ("source_offset".to_string(), json!(source_offset)),
                    ("source_next_offset".to_string(), json!(next_offset)),
                    ("source_total".to_string(), json!(total)),
                ]);
            }
        }
        item
    }).collect();
    stable_response(
        site,
        &token,
        pin,
        json!({"total":filtered.len(),"offset":offset,"limit":limit,"snapshot":token,"counts":counts,"items":items}),
    )
}

/// Stored findings, including retained annotations on restored/orphan identities.
fn reviews(req: &Req<'_>) -> Response {
    let site = req.site;
    let token = match pinned_snapshot(req, Pin::Everything) {
        Ok(token) => token,
        Err(response) => return response,
    };
    let status = req.param("status");
    if !valid_filter(status) && status != "orphan" {
        return Response::error(400);
    }
    let kind = req.param("kind");
    if !matches!(kind, "" | "dead" | "duplicate") {
        return Response::error(400);
    }
    let (offset, limit) = match page(req) {
        Ok(page) => page,
        Err(response) => return response,
    };
    let records = match site.reviews.stored_findings() {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("warning: cannot read stored findings: {error:#}");
            return Response::error(500);
        }
    };
    let query = req.param("q").to_lowercase();
    let candidates: Vec<_> = records
        .iter()
        .filter(|r| {
            (kind.is_empty() || r.kind.value() == kind)
                && (query.is_empty()
                    || r.identity.contains(&query)
                    || r.reason.to_lowercase().contains(&query))
        })
        .map(|r| {
            let fp = site.fingerprint_of(r.kind, &r.identity);
            let state = fp.as_ref().map_or_else(
                || {
                    if r.status == reviews::Status::Pending {
                        State::Pending
                    } else {
                        State::Decided {
                            status: r.status,
                            at: Some(r.at),
                            reason: r.reason.clone(),
                        }
                    }
                },
                |fp| site.state_or_pending(r.kind, &r.identity, fp),
            );
            (r, fp.is_some(), state)
        })
        .collect();
    let counts = state_counts(candidates.iter().map(|(_, _, state)| state.clone()));
    let filtered: Vec<_> = candidates
        .iter()
        .filter(|(_, current, state)| {
            if status == "orphan" {
                !current
            } else {
                matches_status(state, status)
            }
        })
        .collect();
    let items: Vec<_> = filtered
        .iter()
        .skip(offset)
        .take(limit)
        .map(|(r, current, state)| {
            let mut value = record_json(r);
            value["current"] = json!(current);
            value["state"] = state_json(state);
            value["revision"] = json!(site
                .reviews
                .decision_revision(r.kind, &r.identity)
                .unwrap_or(0));
            value
        })
        .collect();
    stable_response(
        site,
        &token,
        Pin::Everything,
        json!({"total":filtered.len(),"offset":offset,"limit":limit,"snapshot":token,"counts":counts,"items":items}),
    )
}

/// History and comments of one finding: `finding?kind=dead&id=<64 hex>`.
fn finding(req: &Req<'_>) -> Response {
    let (Some(kind), identity) = (Kind::parse(req.param("kind")), req.param("id")) else {
        return Response::error(400);
    };
    if !super::reviews::valid_key(identity) {
        return Response::error(400);
    }
    // State and its optimistic revision must come from the same committed read.
    let _review_snapshot = match req.site.reviews.read_snapshot() {
        Ok(snapshot) => snapshot,
        Err(_) => return Response::error(500),
    };
    let known = match kind {
        Kind::Duplicate => req.site.duplicate_ids.contains_key(identity),
        Kind::Dead => req.site.dead_ids.contains_key(identity),
    };
    if !known
        && !req
            .site
            .reviews
            .contains_stored_finding(kind, identity)
            .unwrap_or(false)
    {
        return Response::error(404);
    }
    let fingerprint = req.site.fingerprint_of(kind, identity);
    // A cached analysis fingerprint is not evidence that source is still readable
    // or matches the indexed revision. Reuse the write path's verification.
    let verified = fingerprint.as_ref().is_some_and(|fp| {
        req.site
            .verified_source_fingerprint(kind, identity, fp)
            .is_ok()
    });
    let saved_decision = match req.site.reviews.saved_decision(kind, identity) {
        Ok(record) => record,
        Err(_) => return Response::error(500),
    };
    let state = if known {
        match req.site.reviews.state(
            kind,
            identity,
            fingerprint.as_deref().filter(|_| verified).unwrap_or(""),
        ) {
            Ok(state) => state,
            Err(_) => return Response::error(500),
        }
    } else {
        State::Pending
    };
    let effective_decision = match &state {
        State::Decided { status, .. } => Some(status.value()),
        _ => None,
    };
    let review_validity = if !known {
        "not_current"
    } else if saved_decision
        .as_ref()
        .is_some_and(|r| r.status == reviews::Status::Pending)
    {
        "reset"
    } else if saved_decision.is_none() && matches!(state, State::Pending) {
        "not_reviewed"
    } else if !verified {
        "unavailable"
    } else if matches!(state, State::NeedsReevaluation { .. }) {
        "changed"
    } else {
        "unchanged"
    };
    let actionable = known && !matches!(effective_decision, Some("false_positive" | "ignored"));
    let revision = match req.site.reviews.decision_revision(kind, identity) {
        Ok(r) => r,
        Err(_) => return Response::error(500),
    };
    let (history, comments, files) = match (
        req.site.reviews.history(kind, identity),
        req.site.reviews.comments(kind, identity),
        req.site.reviews.file_reviews(identity),
    ) {
        (Ok(history), Ok(comments), Ok(files)) => (history, comments, files),
        (history, comments, files) => {
            for error in [history.err(), comments.err(), files.err()]
                .into_iter()
                .flatten()
            {
                eprintln!("warning: cannot read finding state: {error:#}");
            }
            return Response::error(500);
        }
    };
    let (members, effective) = match copy_details(req.site, identity, &files) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("warning: cannot read copy details: {error:#}");
            return Response::error(500);
        }
    };
    json_response(&json!({
        "kind": kind.value(), "identity": identity, "current":known,
        "fingerprint":fingerprint,"state":state_json(&state),"revision":revision,
        "saved_decision":saved_decision.as_ref().map(record_json),
        "effective_decision":effective_decision, "review_validity":review_validity,
        "actionable":actionable, "source_verified":verified,
        "members": members, "files": effective,
        "retained_files": files.iter().map(file_review_json).collect::<Vec<_>>(),
        "history": history.iter().map(record_json).collect::<Vec<_>>(),
        "comments": comments.iter().map(|comment| json!({
            "id": comment.id, "body": comment.body,
            "created_at": comment.created_at, "updated_at": comment.updated_at,
        })).collect::<Vec<_>>(),
    }))
}

/// Per-file duplicate labels are working notes, not finding verdicts (AUD-04).
const FILE_LABEL_SCOPE: &str = "file_label";

fn file_review_json(row: &store::FileReview) -> Value {
    json!({
        "path": row.path, "member_key": row.member_key, "status": row.status.value(), "scope": FILE_LABEL_SCOPE,
        "note": row.note, "updated_at": row.updated_at,
    })
}

/// Saved labels stay sparse; descriptors also carry revisions for cleared copies.
fn copy_details(
    site: &Site,
    identity: &str,
    saved: &[store::FileReview],
) -> anyhow::Result<(Vec<Value>, Vec<Value>)> {
    let Some(&group) = site.duplicate_ids.get(identity) else {
        return Ok((Vec::new(), saved.iter().map(file_review_json).collect()));
    };
    let mut fingerprints = reviews::Fingerprints::new(&site.root, &site.navigation);
    let mut current = site.duplicate_keys[group].members.clone();
    for (copy, member) in current
        .iter_mut()
        .zip(&site.duplicates.groups[group].members)
    {
        copy.hash = fingerprints.text_hash(&member.path, member.start_line, member.end_line);
    }
    let copies = member_reviews::resolve(&current, saved);
    let revisions = site.reviews.file_revisions(identity)?;
    let concrete: std::collections::BTreeMap<_, _> = saved
        .iter()
        .map(|row| ((row.path.as_str(), row.member_key.as_str()), row))
        .collect();
    let mut members = Vec::new();
    let mut files = Vec::new();
    for (copy, member) in copies.iter().zip(&site.duplicates.groups[group].members) {
        let revision = store::Store::revision_from(&revisions, identity, &member.path, &copy.key);
        let row = concrete
            .get(&(member.path.as_str(), copy.key.as_str()))
            .or_else(|| concrete.get(&(member.path.as_str(), "")))
            .copied();
        let changed = row
            .and_then(|row| row.code_hash.as_ref())
            .zip(copy.hash.as_ref())
            .is_some_and(|(old, current)| old != current);
        let unavailable = copy.hash.is_none() || copy.uncertain;
        members.push(json!({"member_key":copy.key, "path":member.path, "revision":revision}));
        if let Some(row) = row {
            let mut value = file_review_json(row);
            value["member_key"] = json!(copy.key);
            value["revision"] = json!(revision);
            value["code_changed"] = json!(changed);
            value["code_unavailable"] = json!(unavailable);
            value["inherited"] = json!(row.member_key.is_empty());
            files.push(value);
        }
    }
    Ok((members, files))
}

fn note_kind(name: &str) -> &'static str {
    if name == "index.md" {
        "index"
    } else if name.starts_with("directories/") {
        "directories"
    } else if name.starts_with("files/") {
        "files"
    } else {
        "other"
    }
}

/// Generated notes: `notes?q=&kind=&offset=&limit=`. `enabled` is false when
/// the vault is not generated (`--no-obsidian`).
fn notes(req: &Req<'_>) -> Response {
    let site = req.site;
    let (q, kind) = (req.param("q"), req.param("kind"));
    if !matches!(kind, "" | "index" | "directories" | "files" | "other") {
        return Response::error(400);
    }
    let (offset, limit) = match page(req) {
        Ok(page) => page,
        Err(response) => return response,
    };
    let ids: Vec<usize> = if q.is_empty() {
        (0..site.notes.list.len()).collect()
    } else {
        site.notes.search(q, 5000)
    };
    let ids: Vec<usize> = ids
        .into_iter()
        .filter(|id| kind.is_empty() || note_kind(&site.notes.list[*id].name) == kind)
        .collect();
    let items: Vec<Value> = ids
        .iter()
        .skip(offset)
        .take(limit)
        .map(|id| {
            let note = &site.notes.list[*id];
            json!({
                "name": note.name, "title": note.title, "kind": note_kind(&note.name),
                "source": note.source, "links": note.links.len(),
                "backlinks": site.notes.backlinks[*id].len(),
            })
        })
        .collect();
    json_response(&json!({
        "enabled": !site.notes.list.is_empty(), "total": ids.len(),
        "offset": offset, "limit": limit, "items": items,
    }))
}

/// One note with its raw Markdown (the browser shows it as plain text).
fn note(req: &Req<'_>) -> Response {
    let site = req.site;
    let Some(id) = site.notes.get(req.param("n")) else {
        return Response::error(404);
    };
    let note = &site.notes.list[id];
    let brief = |ids: &[usize]| -> Vec<Value> {
        ids.iter()
            .map(|id| {
                let other = &site.notes.list[*id];
                json!({"name": other.name, "title": other.title})
            })
            .collect()
    };
    json_response(&json!({
        "name": note.name, "title": note.title, "kind": note_kind(&note.name),
        "source": note.source, "text": note.text,
        "links": brief(&note.links), "backlinks": brief(&site.notes.backlinks[id]),
    }))
}

/// Graph of notes: the most connected ones, or the neighbourhood of `n`.
fn graph(req: &Req<'_>) -> Response {
    let notes = &req.site.notes;
    let focus = match req.param("n") {
        "" => None,
        name => match notes.get(name) {
            Some(id) => Some(id),
            None => return Response::error(404),
        },
    };
    let nodes = view_obsidian::graph_nodes(notes, focus);
    let index: std::collections::BTreeMap<usize, usize> =
        nodes.iter().enumerate().map(|(at, id)| (*id, at)).collect();
    let edges: Vec<Value> = nodes
        .iter()
        .flat_map(|id| {
            notes.list[*id]
                .links
                .iter()
                .filter_map(|target| Some(json!([index.get(id)?, index.get(target)?])))
        })
        .collect();
    json_response(&json!({
        "total": notes.list.len(),
        "nodes": nodes.iter().map(|id| json!({
            "name": notes.list[*id].name, "title": notes.list[*id].title,
            "degree": notes.list[*id].links.len() + notes.backlinks[*id].len(),
            "focus": Some(*id) == focus,
        })).collect::<Vec<_>>(),
        "edges": edges,
    }))
}

/// Write routes, all POST with a JSON object body (ADR-019 D6). The caller has
/// already checked origin, content type, size and the CSRF header.
pub(super) const WRITE_ROUTES: [&str; 6] = [
    "api/review",
    "api/review/delete",
    "api/file-review",
    "api/comment",
    "api/comment/update",
    "api/comment/delete",
];

pub(super) fn write(site: &mut Site, route: &str, body: &str) -> Response {
    let Ok(Value::Object(fields)) = serde_json::from_str::<Value>(body) else {
        return Response::error(400);
    };
    let allowed: &[&str] = match route {
        "api/review" => &[
            "kind",
            "id",
            "action",
            "reason",
            "fingerprint",
            "expected_revision",
        ],
        "api/review/delete" => &["kind", "id", "expected_revision"],
        "api/file-review" => &[
            "member_key",
            "id",
            "path",
            "status",
            "note",
            "expected_revision",
            "fingerprint",
        ],
        "api/comment" => &["kind", "id", "body"],
        "api/comment/update" => &["kind", "id", "comment_id", "body", "expected_revision"],
        "api/comment/delete" => &["kind", "id", "comment_id", "expected_revision"],
        _ => return Response::error(404),
    };
    if fields.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Response::error(400);
    }
    // Every write, decisions, labels, deletions and comments alike, is
    // refused while the served snapshot no longer matches the index on disk:
    // the editor must reload what changed before anything is saved (AUD-01).
    if index_stamp(&site.root) != site.index_stamp {
        return Response::error(409);
    }
    let text = |key: &str| fields.get(key).and_then(Value::as_str);
    let comment_id = || fields.get("comment_id").and_then(Value::as_i64);
    let expected_revision = || fields.get("expected_revision").and_then(Value::as_u64);
    let saved = |result: anyhow::Result<Response>| {
        result.unwrap_or_else(|error| {
            if error.downcast_ref::<store::Conflict>().is_some() {
                return Response::error(409);
            }
            eprintln!("warning: cannot save to the dashboard store: {error:#}");
            Response::error(500)
        })
    };
    match route {
        "api/review/delete" => {
            let (Some(kind), Some(identity)) = (text("kind").and_then(Kind::parse), text("id"))
            else {
                return Response::error(400);
            };
            if !reviews::valid_key(identity) {
                return Response::error(400);
            }
            if site.fingerprint_of(kind, identity).is_some() {
                return Response::error(400);
            }
            let Some(revision) = expected_revision() else {
                return Response::error(400);
            };
            saved(
                site.reviews
                    .delete_decision_checked(kind, identity, revision)
                    .map(|found| {
                        if found {
                            json_response(&json!({"deleted": true}))
                        } else {
                            Response::error(404)
                        }
                    }),
            )
        }
        "api/review" => {
            let (Some(kind), Some(identity), Some(status)) = (
                text("kind").and_then(Kind::parse),
                text("id"),
                text("action").and_then(reviews::action_status),
            ) else {
                return Response::error(400);
            };
            if !reviews::valid_key(identity) || fields.get("reason").is_some_and(|r| !r.is_string())
            {
                return Response::error(400);
            }
            let (Some(expected), Some(revision)) = (text("fingerprint"), expected_revision())
            else {
                return Response::error(400);
            };
            let fingerprint = match site.reviewed_fingerprint(kind, identity, expected) {
                Ok(fp) => fp,
                Err(response) => return response,
            };
            let reason = text("reason").unwrap_or_default();
            saved(
                site.reviews
                    .set_checked(
                        (kind, identity, &fingerprint),
                        status,
                        reason,
                        now(),
                        revision,
                    )
                    .map(|()| {
                        if kind == Kind::Dead { site.remember_dead_decision(identity); }
                        let state = site.state_or_pending(kind, identity, &fingerprint);
                        json_response(&json!({"state": state_json(&state),"revision":site.reviews.decision_revision(kind,identity).unwrap_or(0)}))
                    }),
            )
        }
        "api/file-review" => {
            let Some(revision) = expected_revision() else {
                return Response::error(400);
            };
            let (Some(identity), Some(path), Some(status)) = (
                text("id"),
                text("path"),
                text("status").and_then(store::FileStatus::parse),
            ) else {
                return Response::error(400);
            };
            let note = text("note").unwrap_or_default();
            if !reviews::valid_key(identity)
                || fields.get("note").is_some_and(|n| !n.is_string())
                || store::clean_note(note).is_err()
            {
                return Response::error(400);
            }
            let Some(group) = site.duplicate_ids.get(identity).copied() else {
                return Response::error(404);
            };
            if fields
                .get("member_key")
                .is_some_and(|value| !value.is_string())
            {
                return Response::error(400);
            }
            let rows = match site.reviews.file_reviews(identity) {
                Ok(rows) => rows,
                Err(error) => {
                    eprintln!("warning: cannot read copy labels: {error:#}");
                    return Response::error(500);
                }
            };
            let copies = member_reviews::resolve(&site.duplicate_keys[group].members, &rows);
            let matches: Vec<_> = site.duplicates.groups[group]
                .members
                .iter()
                .zip(&copies)
                .filter(|(member, copy)| {
                    member.path == path && text("member_key").is_none_or(|key| copy.key == key)
                })
                .collect();
            let (member, copy) = match matches.as_slice() {
                [] => return Response::error(404),
                [one] => *one,
                _ => return Response::error(409),
            };
            let mut fingerprints = reviews::Fingerprints::new(&site.root, &site.navigation);
            let Some(hash) = fingerprints.text_hash(path, member.start_line, member.end_line)
            else {
                return Response::error(409);
            };
            // Optional: the group content the editor labelled must still be
            // the current, verified content.
            if let Some(expected) = fields.get("fingerprint") {
                let Some(expected) = expected.as_str() else {
                    return Response::error(400);
                };
                if let Err(response) =
                    site.reviewed_fingerprint(Kind::Duplicate, identity, expected)
                {
                    return response;
                }
            }
            saved((|| {
                let revision = if text("member_key").is_some() {
                    site.reviews.set_member_review(
                        identity,
                        &store::FileReview {
                            path: path.to_string(),
                            member_key: copy.key.clone(),
                            member_base: copy.base.clone(),
                            code_hash: Some(hash),
                            status,
                            note: note.to_string(),
                            updated_at: now(),
                        },
                        revision,
                    )?;
                    site.reviews.member_revision(identity, path, &copy.key)?
                } else {
                    // Retain the unique-path contract for older clients/tests.
                    site.reviews.set_verified_file_review(
                        identity,
                        path,
                        status,
                        note,
                        now(),
                        (revision, &hash),
                    )?;
                    site.reviews.file_revision(identity, path)?
                };
                Ok(json_response(&json!({
                    "path":path, "member_key":copy.key, "status":status.value(), "scope":FILE_LABEL_SCOPE,
                    "code_changed":false, "revision":revision,
                    "finding_state":state_json(&site.duplicate_state(group)),
                    "finding_revision":site.reviews.decision_revision(Kind::Duplicate, identity)?,
                })))
            })())
        }
        "api/comment" => {
            let (Some(kind), Some(identity), Some(body)) =
                (text("kind").and_then(Kind::parse), text("id"), text("body"))
            else {
                return Response::error(400);
            };
            if !reviews::valid_key(identity) || store::clean_comment(body).is_err() {
                return Response::error(400);
            }
            if site.fingerprint_of(kind, identity).is_none() {
                return Response::error(404);
            }
            saved(
                site.reviews
                    .add_comment(kind, identity, body, now())
                    .map(|id| json_response(&json!({"comment_id": id}))),
            )
        }
        "api/comment/update" | "api/comment/delete" => {
            let (Some(id), Some(kind), Some(identity), Some(revision)) = (
                comment_id(),
                text("kind").and_then(Kind::parse),
                text("id"),
                expected_revision(),
            ) else {
                return Response::error(400);
            };
            if id <= 0 || !reviews::valid_key(identity) {
                return Response::error(400);
            }
            let result = if route == "api/comment/update" {
                let Some(body) = text("body") else {
                    return Response::error(400);
                };
                if store::clean_comment(body).is_err() {
                    return Response::error(400);
                }
                site.reviews
                    .update_comment(id, (kind, identity, revision), body, now())
            } else {
                site.reviews.delete_comment(id, (kind, identity, revision))
            };
            saved(result.map(|found| {
                if found {
                    json_response(
                        &json!({"comment_id": id, "revision": revision.saturating_add(1)}),
                    )
                } else {
                    Response::error(404)
                }
            }))
        }
        _ => Response::error(404),
    }
}
