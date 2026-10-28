//! Indexed files and symbols, their source previews and related findings.

use serde_json::{json, Value};

use super::findings;
use super::{contains_ci, csv, extension, facet, page_bounds, text, uint, Params};
use crate::web::Site;

const MAX_SOURCE_LINES: usize = 400;
const MAX_REFERENCES: usize = 40;
const MAX_RELATED: usize = 12;

pub(super) fn files(site: &Site) -> Value {
    let rows: Vec<Value> = site
        .navigation
        .files
        .iter()
        .map(|file| {
            let path = file.relative_path.as_str();
            json!([
                path,
                file.language,
                site.symbols.get(path).map_or(0, Vec::len),
                site.groups.get(path).map_or(0, Vec::len),
                site.imports.get(path).map_or(0, Vec::len),
                site.importers.get(path).map_or(0, Vec::len),
            ])
        })
        .collect();
    json!({ "columns": ["path", "language", "symbols", "duplicateGroups", "imports", "importedBy"], "files": rows })
}

fn symbol_row(site: &Site, id: usize) -> Value {
    let symbol = &site.navigation.symbols[id];
    json!({
        "id": id,
        "name": symbol.name,
        "kind": symbol.kind,
        "language": symbol.language,
        "path": symbol.relative_path,
        "line": symbol.line,
        "endLine": symbol.end_line,
        "parent": symbol.parent,
        "refs": site.references.get(&symbol.name).copied().unwrap_or(0),
        "duplicateGroups": site.symbol_groups.get(&id).map_or(0, Vec::len),
        "deadCandidate": site.dead_by_symbol.contains_key(&id),
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Dimension {
    Kind,
    Language,
    Extension,
}

pub(super) fn symbols(site: &Site, params: &Params) -> Value {
    let query = text(params, "q");
    let dir = text(params, "dir").trim_end_matches('/');
    let kinds = csv(params, "kind");
    let languages = csv(params, "language");
    let extensions = csv(params, "ext");
    let min_refs = uint(params, "minRefs", 0);
    let max_refs = uint(params, "maxRefs", usize::MAX);
    let finding = text(params, "finding");
    let base: Vec<usize> = (0..site.navigation.symbols.len())
        .filter(|id| {
            let symbol = &site.navigation.symbols[*id];
            let in_dir = dir.is_empty()
                || symbol.relative_path == dir
                || symbol
                    .relative_path
                    .strip_prefix(dir)
                    .is_some_and(|rest| rest.starts_with('/'));
            in_dir
                && (contains_ci(&symbol.name, query) || contains_ci(&symbol.relative_path, query))
        })
        .collect();
    // A facet counts what the other filters leave, not its own: choosing a
    // kind keeps the other kinds listed with their counts under that choice.
    let keep = |id: usize, skip: Option<Dimension>| {
        let symbol = &site.navigation.symbols[id];
        let refs = site.references.get(&symbol.name).copied().unwrap_or(0);
        (skip == Some(Dimension::Kind) || kinds.is_empty() || kinds.contains(&symbol.kind.as_str()))
            && (skip == Some(Dimension::Language)
                || languages.is_empty()
                || languages.contains(&symbol.language.as_str()))
            && (skip == Some(Dimension::Extension)
                || extensions.is_empty()
                || extensions.contains(&extension(&symbol.relative_path)))
            && refs >= min_refs
            && refs <= max_refs
            && match finding {
                "duplicate" => site.symbol_groups.contains_key(&id),
                "dead-code" => site.dead_by_symbol.contains_key(&id),
                "any" => {
                    site.symbol_groups.contains_key(&id) || site.dead_by_symbol.contains_key(&id)
                }
                "none" => {
                    !site.symbol_groups.contains_key(&id) && !site.dead_by_symbol.contains_key(&id)
                }
                _ => true,
            }
    };
    let facets = json!({
        "kind": facet(base.iter().filter(|id| keep(**id, Some(Dimension::Kind))).map(|id| site.navigation.symbols[*id].kind.as_str())),
        "language": facet(base.iter().filter(|id| keep(**id, Some(Dimension::Language))).map(|id| site.navigation.symbols[*id].language.as_str())),
        "ext": facet(base.iter().filter(|id| keep(**id, Some(Dimension::Extension))).map(|id| extension(&site.navigation.symbols[*id].relative_path))),
    });
    let mut ids: Vec<usize> = base.into_iter().filter(|id| keep(*id, None)).collect();
    let sort = text(params, "sort");
    let refs_of = |id: &usize| {
        site.references
            .get(&site.navigation.symbols[*id].name)
            .copied()
            .unwrap_or(0)
    };
    ids.sort_by(|a, b| {
        let (x, y) = (&site.navigation.symbols[*a], &site.navigation.symbols[*b]);
        let order = match sort {
            "kind" => x.kind.cmp(&y.kind),
            "language" => x.language.cmp(&y.language),
            "path" => x
                .relative_path
                .cmp(&y.relative_path)
                .then(x.line.cmp(&y.line)),
            "line" => x.line.cmp(&y.line),
            "refs" => refs_of(a).cmp(&refs_of(b)),
            _ => x.name.to_lowercase().cmp(&y.name.to_lowercase()),
        };
        order.then(a.cmp(b))
    });
    if text(params, "order") == "desc" {
        ids.reverse();
    }
    let (page, size, start) = page_bounds(params, ids.len(), 50);
    let items: Vec<Value> = ids
        .iter()
        .skip(start)
        .take(size)
        .map(|id| symbol_row(site, *id))
        .collect();
    json!({ "total": ids.len(), "page": page, "size": size, "items": items, "facets": facets })
}

/// Why the lines of an indexed file are not served.
pub(super) enum Unavailable {
    /// Not in the snapshot, outside the project, or unreadable.
    Missing,
    /// On disk it no longer matches the content the index parsed, so the
    /// snapshot's line numbers would point into different text.
    Changed,
}

/// Reads `start..=end` (1-based) of an indexed file. Only paths present in the
/// snapshot are readable, symlinks leaving the project are refused, and the
/// file must still hash to what the index saw.
pub(super) fn read_indexed_lines(
    site: &Site,
    path: &str,
    start: usize,
    end: usize,
) -> Result<(Vec<String>, bool), Unavailable> {
    let position = site
        .navigation
        .files
        .binary_search_by(|file| file.relative_path.as_str().cmp(path))
        .map_err(|_| Unavailable::Missing)?;
    let full = site.root.join(path);
    let canonical = full.canonicalize().map_err(|_| Unavailable::Missing)?;
    if !canonical.starts_with(&site.root) {
        return Err(Unavailable::Missing);
    }
    let bytes = mct_index::read_repository_file(&canonical).map_err(|_| Unavailable::Missing)?;
    let hash =
        git2::Oid::hash_object(git2::ObjectType::Blob, &bytes).map_err(|_| Unavailable::Missing)?;
    if hash.to_string() != site.navigation.files[position].content_hash {
        return Err(Unavailable::Changed);
    }
    let content = String::from_utf8_lossy(&bytes);
    let start = start.max(1);
    let end = end.max(start).min(start + MAX_SOURCE_LINES - 1);
    let total = content.lines().count();
    let lines = content
        .lines()
        .skip(start - 1)
        .take(end - start + 1)
        .map(str::to_string)
        .collect();
    Ok((lines, end < total && end - start + 1 == MAX_SOURCE_LINES))
}

/// [`read_indexed_lines`] for callers that treat a changed file like a missing one.
pub(super) fn read_lines(
    site: &Site,
    path: &str,
    start: usize,
    end: usize,
) -> Option<(Vec<String>, bool)> {
    read_indexed_lines(site, path, start, end).ok()
}

pub(super) fn source(site: &Site, params: &Params) -> Result<Value, (u16, &'static str)> {
    let path = text(params, "path");
    let start = uint(params, "start", 1);
    let end = uint(params, "end", start + 79);
    let (lines, truncated) =
        read_indexed_lines(site, path, start, end).map_err(|why| match why {
            Unavailable::Missing => (404, "source not available"),
            Unavailable::Changed => (
                409,
                "this file changed since it was indexed; reindex and restart the dashboard",
            ),
        })?;
    let language = site
        .navigation
        .files
        .iter()
        .find(|file| file.relative_path == path)
        .map(|file| file.language.as_str());
    Ok(
        json!({ "path": path, "start": start.max(1), "lines": lines, "truncated": truncated, "language": language }),
    )
}

pub(super) fn symbol(site: &Site, params: &Params) -> Result<Value, (u16, &'static str)> {
    let id = uint(params, "id", usize::MAX);
    let symbol = site
        .navigation
        .symbols
        .get(id)
        .ok_or((404, "unknown symbol"))?;
    let end = symbol
        .end_line
        .map_or(symbol.line as usize + 39, |end| end as usize);
    let code = read_lines(site, &symbol.relative_path, symbol.line as usize, end);
    let references: Vec<Value> = site
        .index
        .find_references(&symbol.name)
        .unwrap_or_default()
        .into_iter()
        .take(MAX_REFERENCES)
        .map(|hit| {
            json!({ "kind": hit.kind, "from": hit.from_symbol, "path": hit.relative_path, "line": hit.line })
        })
        .collect();
    let related: Vec<Value> = site
        .symbols
        .get(&symbol.relative_path)
        .into_iter()
        .flatten()
        .filter(|other| **other != id)
        .filter(|other| {
            let other = &site.navigation.symbols[**other];
            other.parent == symbol.parent || Some(&other.name) == symbol.parent.as_ref()
        })
        .take(MAX_RELATED)
        .map(|other| symbol_row(site, *other))
        .collect();
    let groups: Vec<Value> = site
        .symbol_groups
        .get(&id)
        .into_iter()
        .flatten()
        .map(|group| findings::duplicate_row(site, *group))
        .collect();
    let dead = site
        .dead_by_symbol
        .get(&id)
        .map(|index| findings::dead_row(site, *index));
    let notes: Vec<Value> = site
        .vault
        .notes
        .iter()
        .filter(|note| note.code_path() == Some(symbol.relative_path.as_str()))
        .map(|note| json!({ "path": note.path, "title": note.title }))
        .collect();
    Ok(json!({
        "symbol": symbol_row(site, id),
        "code": code.as_ref().map(|(lines, truncated)| json!({ "start": symbol.line, "lines": lines, "truncated": truncated })),
        "references": references,
        "referenceNote": "References are matched by name over the index; same-named symbols share a count.",
        "related": related,
        "duplicates": groups,
        "dead": dead,
        "notes": notes,
        "imports": site.imports.get(&symbol.relative_path).cloned().unwrap_or_default(),
        "importedBy": site.importers.get(&symbol.relative_path).cloned().unwrap_or_default(),
    }))
}
