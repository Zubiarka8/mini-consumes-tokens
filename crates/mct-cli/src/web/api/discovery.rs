//! Keyword search across every indexed surface, and the module/file dependency graph.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Instant;

use mct_index::QueryScope;
use serde_json::{json, Value};

use super::{contains_ci, csv, text, uint, Params};
use crate::web::Site;

const GROUP_LIMIT_ALL: usize = 8;
const GROUP_LIMIT_SCOPE: usize = 100;
const MAX_NODES: usize = 150;
const MAX_EXTERNAL: usize = 200;

fn symbol_id(site: &Site, path: &str, name: &str, line: u32) -> Option<usize> {
    site.symbols
        .get(path)?
        .iter()
        .copied()
        .find(|id| {
            let symbol = &site.navigation.symbols[*id];
            symbol.name == name && symbol.line == line
        })
}

fn group(total: usize, items: Vec<Value>, limit: usize) -> Value {
    json!({ "total": total, "items": items.into_iter().take(limit).collect::<Vec<_>>() })
}

pub(super) fn search(site: &Site, params: &Params) -> Value {
    let started = Instant::now();
    let query = text(params, "q");
    let scope = match text(params, "scope") {
        "" => "all",
        other => other,
    };
    let languages = csv(params, "language");
    let kinds = csv(params, "kind");
    let limit = if scope == "all" { GROUP_LIMIT_ALL } else { GROUP_LIMIT_SCOPE };
    let wants = |name: &str| scope == "all" || scope == name;
    let language_ok = |language: &str| languages.is_empty() || languages.contains(&language);
    let mut groups = serde_json::Map::new();
    if query.is_empty() {
        return json!({ "query": query, "mode": "keyword", "semantic": false, "tookMs": 0, "groups": groups });
    }

    if wants("symbols") {
        let mut seen = BTreeSet::new();
        let mut items = Vec::new();
        let hits = site.index.search_symbols(query, QueryScope::default()).unwrap_or_default();
        let exact = site.navigation.symbols.iter().filter(|s| contains_ci(&s.name, query));
        let candidates = hits
            .iter()
            .map(|h| (h.name.as_str(), h.kind.as_str(), h.language.as_str(), h.relative_path.as_str(), h.line, h.parent.as_deref()))
            .chain(exact.map(|s| (s.name.as_str(), s.kind.as_str(), s.language.as_str(), s.relative_path.as_str(), s.line, s.parent.as_deref())));
        for (name, kind, language, path, line, parent) in candidates {
            if !language_ok(language) || !(kinds.is_empty() || kinds.contains(&kind)) {
                continue;
            }
            if !seen.insert((path.to_string(), name.to_string(), line)) {
                continue;
            }
            let id = symbol_id(site, path, name, line);
            let refs = site.references.get(name).copied().unwrap_or(0);
            items.push(json!({ "type": "symbol", "title": name, "kind": kind, "language": language, "path": path,
                "line": line, "snippet": parent.map_or_else(|| kind.to_string(), |p| format!("{kind} en {p}")),
                "symbolId": id, "refs": refs }));
        }
        groups.insert("symbols".into(), group(items.len(), items, limit));
    }
    if wants("files") || wants("dependencies") {
        let items: Vec<Value> = site
            .navigation
            .files
            .iter()
            .filter(|file| language_ok(&file.language) && contains_ci(&file.relative_path, query))
            .map(|file| {
                let path = file.relative_path.as_str();
                json!({ "type": "file", "title": path.rsplit('/').next().unwrap_or(path), "path": path, "language": file.language,
                    "snippet": path, "symbols": site.symbols.get(path).map_or(0, Vec::len),
                    "imports": site.imports.get(path).map_or(0, Vec::len),
                    "importedBy": site.importers.get(path).map_or(0, Vec::len) })
            })
            .collect();
        if wants("files") {
            groups.insert("files".into(), group(items.len(), items.clone(), limit));
        }
        if wants("dependencies") {
            let linked: Vec<Value> = items
                .into_iter()
                .filter(|item| item["imports"].as_u64().unwrap_or(0) + item["importedBy"].as_u64().unwrap_or(0) > 0)
                .map(|mut item| {
                    item["type"] = json!("dependency");
                    item
                })
                .collect();
            groups.insert("dependencies".into(), group(linked.len(), linked, limit));
        }
    }
    if wants("code") {
        let scope = QueryScope::default();
        let hits = site.index.search_literals(query, scope).unwrap_or_default();
        let items: Vec<Value> = hits
            .iter()
            .filter(|hit| language_ok(&hit.language))
            .map(|hit| json!({ "type": "code", "title": hit.symbol.as_ref().map_or(hit.relative_path.as_str(), |(name, _)| name.as_str()),
                "path": hit.relative_path, "line": hit.line, "language": hit.language, "snippet": hit.text,
                "note": "literal" }))
            .collect();
        groups.insert("code".into(), group(items.len(), items, limit));
    }
    if wants("duplicates") || wants("deadCode") {
        let rows = super::findings::list(site, &Params::from([("size".into(), "200".into()), ("q".into(), query.into())]));
        let rows = rows["items"].as_array().cloned().unwrap_or_default();
        for (name, kind) in [("duplicates", "duplicate"), ("deadCode", "dead-code")] {
            if wants(name) {
                let items: Vec<Value> = rows
                    .iter()
                    .filter(|row| row["type"] == kind && language_ok(row["language"].as_str().unwrap_or("")))
                    .map(|row| json!({ "type": kind, "title": row["title"], "path": row["path"], "line": row["line"],
                        "language": row["language"], "key": row["key"], "state": row["state"],
                        "snippet": row["path"] }))
                    .collect();
                groups.insert(name.into(), group(items.len(), items, limit));
            }
        }
    }
    if wants("obsidian") {
        let search_bodies = site.vault.notes.len() <= 500;
        let items: Vec<Value> = site
            .vault
            .notes
            .iter()
            .filter_map(|note| {
                let in_meta = contains_ci(&note.title, query)
                    || contains_ci(&note.path, query)
                    || note.tags.iter().any(|tag| contains_ci(tag, query));
                let body_hit = (!in_meta && search_bodies)
                    .then(|| crate::web::obsidian::read_text(&site.vault_dir.join(&note.path)))
                    .flatten()
                    .and_then(|text| {
                        let (_, body) = crate::web::obsidian::split_frontmatter(&text);
                        body.lines().find(|line| contains_ci(line, query)).map(|line| line.trim().to_string())
                    });
                (in_meta || body_hit.is_some()).then(|| json!({ "type": "note", "title": note.title, "path": note.path,
                    "snippet": body_hit.unwrap_or_else(|| note.path.clone()), "tags": note.tags }))
            })
            .collect();
        groups.insert("obsidian".into(), group(items.len(), items, limit));
    }
    json!({ "query": query, "mode": "keyword", "semantic": false, "tookMs": started.elapsed().as_millis(), "groups": groups })
}

/// Directory-level module of a file: its first `depth` directory segments.
fn module_of(path: &str, depth: usize) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let take = depth.min(parts.len().saturating_sub(1));
    if take == 0 {
        "(raíz)".to_string()
    } else {
        parts[..take].join("/")
    }
}

pub(super) fn dependencies(site: &Site, params: &Params) -> Value {
    let level = if text(params, "level") == "file" { "file" } else { "dir" };
    let depth = uint(params, "group", 2).clamp(1, 6);
    let hops = uint(params, "depth", 2).clamp(1, 6);
    let focus = text(params, "focus");
    let node_of = |path: &str| if level == "file" { path.to_string() } else { module_of(path, depth) };
    let languages = csv(params, "language");
    let file_language: BTreeMap<&str, &str> = site
        .navigation
        .files
        .iter()
        .map(|file| (file.relative_path.as_str(), file.language.as_str()))
        .collect();
    let mut weights: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut members: BTreeMap<String, usize> = BTreeMap::new();
    let mut node_language: BTreeMap<String, &str> = BTreeMap::new();
    for (path, language) in &file_language {
        let node = node_of(path);
        *members.entry(node.clone()).or_default() += 1;
        node_language.entry(node).or_insert(language);
    }
    let mut file_edges = Vec::new();
    for (from, targets) in &site.imports {
        for to in targets {
            file_edges.push((from.clone(), to.clone()));
            let (a, b) = (node_of(from), node_of(to));
            if a != b {
                *weights.entry((a, b)).or_default() += 1;
            }
        }
    }
    let cycles = mct_index::find_cycles(&file_edges);
    let in_cycle: BTreeSet<String> = cycles.iter().flatten().map(|path| node_of(path)).collect();
    let mut out_degree: BTreeMap<&str, usize> = BTreeMap::new();
    let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
    for ((from, to), weight) in &weights {
        *out_degree.entry(from).or_default() += weight;
        *in_degree.entry(to).or_default() += weight;
    }
    let focus_node = (!focus.is_empty()).then(|| node_of(focus));
    let selected: BTreeSet<String> = match &focus_node {
        Some(start) if members.contains_key(start) => {
            let mut seen = BTreeSet::from([start.clone()]);
            let mut queue = VecDeque::from([(start.clone(), 0)]);
            while let Some((node, distance)) = queue.pop_front() {
                if distance == hops {
                    continue;
                }
                for ((from, to), _) in &weights {
                    let next = if *from == node { to } else if *to == node { from } else { continue };
                    if seen.len() < MAX_NODES && seen.insert(next.clone()) {
                        queue.push_back((next.clone(), distance + 1));
                    }
                }
            }
            seen
        }
        _ => {
            let mut ranked: Vec<&String> = members.keys().collect();
            ranked.sort_by_key(|node| {
                std::cmp::Reverse(out_degree.get(node.as_str()).unwrap_or(&0) + in_degree.get(node.as_str()).unwrap_or(&0))
            });
            ranked.into_iter().take(MAX_NODES).cloned().collect()
        }
    };
    let nodes: Vec<Value> = selected
        .iter()
        .filter(|node| languages.is_empty() || languages.contains(node_language.get(node.as_str()).unwrap_or(&"")))
        .map(|node| {
            let imports = out_degree.get(node.as_str()).copied().unwrap_or(0);
            let importers = in_degree.get(node.as_str()).copied().unwrap_or(0);
            json!({ "id": node, "label": node.rsplit('/').next().unwrap_or(node), "kind": level,
                "language": node_language.get(node.as_str()), "files": members.get(node), "imports": imports,
                "importers": importers, "cycle": in_cycle.contains(node), "unused": importers == 0 })
        })
        .collect();
    let shown: BTreeSet<&str> = nodes.iter().filter_map(|node| node["id"].as_str()).collect();
    let edges: Vec<Value> = weights
        .iter()
        .filter(|((from, to), _)| shown.contains(from.as_str()) && shown.contains(to.as_str()))
        .map(|((from, to), weight)| json!({ "from": from, "to": to, "weight": weight }))
        .collect();
    let external: Vec<Value> = site
        .index
        .status()
        .map(|status| status.dependencies)
        .unwrap_or_default()
        .into_iter()
        .map(|manifest| {
            let dependencies: Vec<Value> = manifest
                .dependencies
                .iter()
                .take(MAX_EXTERNAL)
                .map(|dep| json!({ "name": dep.name, "version": dep.version }))
                .collect();
            json!({ "manifest": manifest.manifest_path, "language": manifest.language,
                    "count": manifest.dependencies.len(), "dependencies": dependencies })
        })
        .collect();
    json!({
        "level": level, "group": depth, "focus": focus_node, "hops": hops,
        "totalNodes": members.len(), "totalEdges": weights.len(),
        "truncated": selected.len() >= MAX_NODES && members.len() > MAX_NODES,
        "nodes": nodes, "edges": edges,
        "cycles": cycles.iter().take(50).collect::<Vec<_>>(),
        "external": external,
        "edgeKinds": ["imports"],
    })
}
