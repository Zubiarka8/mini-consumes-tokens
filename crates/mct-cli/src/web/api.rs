//! Read-only JSON API over the in-memory snapshot, plus the review POST body.
//!
//! Every value comes from the index, the duplicate/dead-code analysis, the
//! review file or the generated vault. Capabilities the backend does not have
//! are reported as `null`/`false` so the UI can say so instead of inventing data.

mod code;
mod discovery;
mod findings;
mod notes;
mod overview;

use std::collections::BTreeMap;
use std::time::Instant;

use serde_json::{json, Value};

use super::response::Response;
use super::Site;

pub(super) use findings::{review_json, review_sources_current};

pub(super) type Params = BTreeMap<String, String>;

const MAX_SAMPLES: usize = 500;

pub(super) fn text<'a>(params: &'a Params, key: &str) -> &'a str {
    params.get(key).map_or("", |value| value.trim())
}

pub(super) fn uint(params: &Params, key: &str, default: usize) -> usize {
    params
        .get(key)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// Comma-separated multi-value parameter; empty means "no filter".
pub(super) fn csv<'a>(params: &'a Params, key: &str) -> Vec<&'a str> {
    text(params, key)
        .split(',')
        .filter(|value| !value.is_empty())
        .collect()
}

pub(super) fn contains_ci(haystack: &str, needle: &str) -> bool {
    needle.is_empty() || haystack.to_lowercase().contains(&needle.to_lowercase())
}

pub(super) fn extension(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map_or("", |(_, ext)| ext)
}

/// `[[value, count], …]` sorted by count then value.
pub(super) fn facet<'a>(values: impl IntoIterator<Item = &'a str>) -> Value {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    let mut pairs: Vec<_> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    Value::Array(pairs.into_iter().map(|(k, n)| json!([k, n])).collect())
}

pub(super) fn page_bounds(
    params: &Params,
    total: usize,
    default_size: usize,
) -> (usize, usize, usize) {
    let size = uint(params, "size", default_size).clamp(1, 200);
    let last = total.saturating_sub(1) / size;
    let page = uint(params, "page", 0).min(last);
    (page, size, page * size)
}

pub(super) fn route(site: &Site, route: &str, params: &Params) -> Option<Response> {
    let name = route.strip_prefix("api/")?;
    let started = Instant::now();
    let (label, value): (&'static str, Result<Value, (u16, &'static str)>) = match name {
        "meta" => ("meta", Ok(overview::meta(site))),
        "summary" => ("summary", Ok(overview::summary(site))),
        "stats" => ("stats", Ok(overview::stats(site))),
        "files" => ("files", Ok(code::files(site))),
        "symbols" => ("symbols", Ok(code::symbols(site, params))),
        "symbol" => ("symbol", code::symbol(site, params)),
        "source" => ("source", code::source(site, params)),
        "findings" => ("findings", Ok(findings::list(site, params))),
        "finding" => ("finding", findings::detail(site, params)),
        "search" => ("search", Ok(discovery::search(site, params))),
        "dependencies" => ("dependencies", Ok(discovery::dependencies(site, params))),
        "notes" => ("notes", Ok(notes::list(site, params))),
        "note" => ("note", notes::detail(site, params)),
        "notes-graph" => ("notes-graph", Ok(notes::graph(site, params))),
        "ignore" => ("ignore", site.ignore_state(params)),
        _ => return None,
    };
    let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    {
        let mut latency = site.latency.borrow_mut();
        let samples = latency.entry(label).or_default();
        if samples.len() < MAX_SAMPLES {
            samples.push(micros);
        }
    }
    Some(match value {
        Ok(value) => Response::json(&value),
        Err((status, message)) => Response::failure(status, message),
    })
}
