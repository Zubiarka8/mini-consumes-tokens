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
const MAX_GRAPH_NODES: usize = 400;
const MAX_SYMBOLS: usize = 30;

fn folder(note: &Note) -> &str {
    note.path.rsplit_once('/').map_or("(raíz)", |(dir, _)| dir)
}

fn backlink_count(site: &Site, note: &Note) -> usize {
    site.vault.backlinks.get(&note.path).map_or(0, Vec::len)
}

/// The indexed file a note was generated for, if it still exists in the index.
fn code_file<'a>(site: &Site, note: &'a Note) -> Option<&'a str> {
    note.code_path().filter(|path| {
        site.symbols.contains_key(*path)
            || site
                .navigation
                .files
                .iter()
                .any(|f| f.relative_path == *path)
    })
}

fn has_dead(site: &Site, path: &str) -> bool {
    site.dead
        .iter()
        .any(|candidate| site.navigation.symbols[candidate.symbol].relative_path == path)
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
                    "dead-code" => code_file(site, note).is_some_and(|path| has_dead(site, path)),
                    _ => true,
                })
        })
        .collect();
    let sort = text(params, "sort");
    notes.sort_by(|a, b| {
        match sort {
            "modified" => a.modified.cmp(&b.modified),
            "backlinks" => backlink_count(site, a).cmp(&backlink_count(site, b)),
            "symbols" => a.symbol_count().cmp(&b.symbol_count()),
            _ => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        }
        .then(a.path.cmp(&b.path))
    });
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
            "all": site.vault.notes.len(), "truncated": site.vault.truncated })
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
                if seen.len() < MAX_GRAPH_NODES
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
            .take(MAX_GRAPH_NODES)
            .map(|n| n.path.as_str())
            .collect()
    };
    let nodes: Vec<Value> = selected
        .iter()
        .filter_map(|path| by_path.get(path))
        .map(|note| json!({ "id": note.path, "label": note.title, "folder": folder(note), "tags": note.tags,
                            "degree": degree(note), "orphan": degree(note) == 0, "hasCode": code_file(site, note).is_some() }))
        .collect();
    let edges: Vec<Value> = selected
        .iter()
        .filter_map(|path| by_path.get(path))
        .flat_map(|note| {
            note.links
                .iter()
                .filter(|t| selected.contains(t.as_str()))
                .map(move |t| json!({ "from": note.path, "to": t }))
        })
        .collect();
    json!({ "nodes": nodes, "edges": edges, "total": site.vault.notes.len(),
            "truncated": selected.len() >= MAX_GRAPH_NODES, "edgeKinds": ["markdown-link"] })
}
