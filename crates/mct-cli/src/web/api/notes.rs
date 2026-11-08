//! Notes of the generated Obsidian vault: list, one note, and the link graph.
//!
//! Code relations come only from the `path` front-matter key MCT writes into
//! its own notes, and are checked against the index before being reported.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde_json::{json, Value};

use super::{contains_ci, csv, facet, page_bounds, text, uint, Params};
use crate::web::obsidian::{self, Note};
use crate::web::Site;

const DAY: u64 = 86_400;
/// Safety cap for the graph; `all=1` lifts it.
const MAX_GRAPH_NODES: usize = 5000;
const MAX_SYMBOLS: usize = 30;

fn folder(note: &Note) -> &str {
    note.path.rsplit_once('/').map_or("(raíz)", |(dir, _)| dir)
}

fn backlink_count(site: &Site, note: &Note) -> usize {
    site.vault.backlinks.get(&note.path).map_or(0, Vec::len)
}

/// The indexed file a note was generated for, if it still exists in the index.
fn code_file<'a>(site: &Site, note: &'a Note) -> Option<&'a str> {
    // `navigation.files` is sorted by path when the snapshot loads.
    note.code_path().filter(|path| {
        site.symbols.contains_key(*path)
            || site
                .navigation
                .files
                .binary_search_by(|f| f.relative_path.as_str().cmp(path))
                .is_ok()
    })
}

/// Files with at least one dead-code candidate.
fn dead_files(site: &Site) -> BTreeSet<&str> {
    site.dead
        .iter()
        .map(|candidate| {
            site.navigation.symbols[candidate.symbol]
                .relative_path
                .as_str()
        })
        .collect()
}

fn note_row(site: &Site, note: &Note) -> Value {
    json!({
        "path": note.path, "title": note.title, "tags": note.tags, "folder": folder(note),
        "modified": note.modified, "bytes": note.bytes,
        "codePath": code_file(site, note),
        "symbolCount": note.symbol_count(),
        "backlinks": backlink_count(site, note),
        "links": note.links.len(),
    })
}

pub(super) fn list(site: &Site, params: &Params) -> Value {
    let query = text(params, "q");
    let folders = csv(params, "folder");
    let tags = csv(params, "tag");
    let relations = csv(params, "rel");
    let days = uint(params, "days", 0) as u64;
    let now = site.generated;
    let base: Vec<&Note> = site
        .vault
        .notes
        .iter()
        .filter(|note| {
            contains_ci(&note.title, query)
                || contains_ci(&note.path, query)
                || note.tags.iter().any(|tag| contains_ci(tag, query))
        })
        .collect();
    let facets = json!({
        "folder": facet(base.iter().map(|note| folder(note))),
        "tag": facet(base.iter().flat_map(|note| note.tags.iter().map(String::as_str))),
    });
    // Built once per request, and only when the filter asks for it.
    let dead = if relations.contains(&"dead-code") {
        dead_files(site)
    } else {
        BTreeSet::new()
    };
    let mut notes: Vec<&Note> = base
        .into_iter()
        .filter(|note| {
            (folders.is_empty() || folders.contains(&folder(note)))
                && (tags.is_empty() || note.tags.iter().any(|tag| tags.contains(&tag.as_str())))
                && (days == 0 || note.modified + days * DAY >= now)
                && relations.iter().all(|relation| match *relation {
                    "with-code" => code_file(site, note).is_some(),
                    "no-code" => code_file(site, note).is_none(),
                    "with-backlinks" => backlink_count(site, note) > 0,
                    "no-backlinks" => backlink_count(site, note) == 0,
                    "duplicates" => {
                        code_file(site, note).is_some_and(|path| site.groups.contains_key(path))
                    }
                    "dead-code" => code_file(site, note).is_some_and(|path| dead.contains(path)),
                    _ => true,
                })
        })
        .collect();
    let sort = text(params, "sort");
    // Keys are computed once per note, not once per comparison; ties by path.
    match sort {
        "modified" => notes.sort_by_key(|note| (note.modified, note.path.as_str())),
        "backlinks" => {
            notes.sort_by_cached_key(|note| (backlink_count(site, note), note.path.as_str()));
        }
        "symbols" => notes.sort_by_cached_key(|note| (note.symbol_count(), note.path.as_str())),
        _ => notes.sort_by_cached_key(|note| (note.title.to_lowercase(), note.path.as_str())),
    }
    if text(params, "order") == "desc" {
        notes.reverse();
    }
    let (page, size, start) = page_bounds(params, notes.len(), 50);
    let items: Vec<Value> = notes
        .iter()
        .skip(start)
        .take(size)
        .map(|note| note_row(site, note))
        .collect();
    json!({ "total": notes.len(), "page": page, "size": size, "items": items, "facets": facets,
            "all": site.vault.notes.len(), "truncated": site.vault.truncated, "skipped": site.vault.skipped })
}

pub(super) fn detail(site: &Site, params: &Params) -> Result<Value, (u16, &'static str)> {
    let path = text(params, "path");
    let note = site
        .vault
        .notes
        .iter()
        .find(|note| note.path == path)
        .ok_or((404, "unknown note"))?;
    let titles: BTreeMap<&str, &str> = site
        .vault
        .notes
        .iter()
        .map(|n| (n.path.as_str(), n.title.as_str()))
        .collect();
    let link = |target: &String| json!({ "path": target, "title": titles.get(target.as_str()) });
    let raw = obsidian::read_text(&site.vault_dir.join(&note.path)).unwrap_or_default();
    let (frontmatter, body) = obsidian::split_frontmatter(&raw);
    let code = code_file(site, note);
    let symbols: Vec<Value> = code
        .and_then(|path| site.symbols.get(path))
        .into_iter()
        .flatten()
        .take(MAX_SYMBOLS)
        .map(|id| {
            let symbol = &site.navigation.symbols[*id];
            json!({ "id": id, "name": symbol.name, "kind": symbol.kind, "line": symbol.line })
        })
        .collect();
    let duplicate_keys: Vec<&String> = code
        .and_then(|path| site.groups.get(path))
        .into_iter()
        .flatten()
        .map(|id| &site.review_keys[*id])
        .collect();
    let dead_keys: Vec<&String> = site
        .dead
        .iter()
        .filter(|candidate| {
            Some(
                site.navigation.symbols[candidate.symbol]
                    .relative_path
                    .as_str(),
            ) == code
        })
        .map(|candidate| &candidate.key)
        .collect();
    let mut row = note_row(site, note);
    row["frontmatter"] = frontmatter.iter().map(|(k, v)| json!([k, v])).collect();
    row["body"] = json!(body);
    row["linksTo"] = note.links.iter().map(link).collect();
    row["backlinkList"] = site
        .vault
        .backlinks
        .get(&note.path)
        .into_iter()
        .flatten()
        .map(link)
        .collect();
    row["symbols"] = Value::Array(symbols);
    row["duplicateKeys"] = json!(duplicate_keys);
    row["deadCodeKeys"] = json!(dead_keys);
    row["relationSource"] = json!("front matter `path`, verified against the index");
    Ok(row)
}

pub(super) fn graph(site: &Site, params: &Params) -> Value {
    let folders = csv(params, "folder");
    let tags = csv(params, "tag");
    let focus = text(params, "focus");
    let hops = uint(params, "depth", 2).clamp(1, 6);
    let orphans = text(params, "orphans");
    let max_nodes = if text(params, "all") == "1" {
        usize::MAX
    } else {
        MAX_GRAPH_NODES
    };
    let by_path: BTreeMap<&str, &Note> = site
        .vault
        .notes
        .iter()
        .map(|n| (n.path.as_str(), n))
        .collect();
    let degree = |note: &Note| note.links.len() + backlink_count(site, note);
    let eligible = |note: &&Note| {
        (folders.is_empty() || folders.contains(&folder(note)))
            && (tags.is_empty() || note.tags.iter().any(|tag| tags.contains(&tag.as_str())))
            && match orphans {
                "only" => degree(note) == 0,
                "hide" => degree(note) > 0,
                _ => true,
            }
    };
    let selected: BTreeSet<&str> = if let Some(start) = by_path.get(focus) {
        let mut seen = BTreeSet::from([start.path.as_str()]);
        let mut queue = VecDeque::from([(start.path.as_str(), 0)]);
        while let Some((path, distance)) = queue.pop_front() {
            if distance == hops {
                continue;
            }
            let outgoing = by_path.get(path).into_iter().flat_map(|n| n.links.iter());
            let incoming = site.vault.backlinks.get(path).into_iter().flatten();
            for next in outgoing.chain(incoming) {
                if seen.len() < max_nodes
                    && by_path.get(next.as_str()).is_some_and(eligible)
                    && seen.insert(next)
                {
                    queue.push_back((next, distance + 1));
                }
            }
        }
        seen
    } else {
        let mut notes: Vec<&Note> = site.vault.notes.iter().filter(eligible).collect();
        notes.sort_by_key(|note| std::cmp::Reverse(degree(note)));
        notes
            .into_iter()
            .take(max_nodes)
            .map(|n| n.path.as_str())
            .collect()
    };
    let shown: Vec<&Note> = selected
        .iter()
        .filter_map(|path| by_path.get(path).copied())
        .collect();
    let nodes: Vec<Value> = shown
        .iter()
        .map(|note| json!({ "id": note.path, "label": note.title, "folder": folder(note), "tags": note.tags,
                            "degree": degree(note), "orphan": degree(note) == 0, "hasCode": code_file(site, note).is_some() }))
        .collect();
    // Each edge is `[from, to]`, positions in `nodes`: note paths are long and
    // would otherwise be repeated in every edge (most of the payload).
    let position: BTreeMap<&str, usize> = shown
        .iter()
        .enumerate()
        .map(|(at, note)| (note.path.as_str(), at))
        .collect();
    let edges: Vec<Value> = shown
        .iter()
        .enumerate()
        .flat_map(|(from, note)| {
            note.links
                .iter()
                .filter_map(|target| position.get(target.as_str()))
                .map(move |to| json!([from, to]))
        })
        .collect();
    json!({ "nodes": nodes, "edges": edges, "total": site.vault.notes.len(),
            "truncated": site.vault.notes.len() > max_nodes, "edgeKinds": ["markdown-link"] })
}
