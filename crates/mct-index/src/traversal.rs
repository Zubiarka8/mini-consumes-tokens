//! Multi-hop BFS over the relation graph, layered on top of `Index`'s
//! existing single-hop queries rather than a recursive SQL query.
//! This keeps the SQL in `queries.rs` simple and makes cycle-safety (a
//! visited-set of symbol ids) trivial to reason about and test, at the
//! cost of one query per node instead of one recursive query — an
//! acceptable trade for the repo sizes this project targets.
//!
//! Hop 1 is the name-based query: every relation spelled (or made by a
//! symbol named) the start name, minus incoming relations proven to target a
//! same-named definition outside the (possibly scoped) start. Every later
//! hop walks symbol ids, and only along edges whose target is proven: a
//! forward hop follows a [`Resolution::Resolved`] callee, a backward hop
//! continues only from a caller whose relation resolved to the node. An
//! ambiguous or unresolved relation is reported where it is found, but never
//! expanded, so a walk can't wander into a same-named unrelated definition.
//!
//! Pagination happens in SQL: rows before the page are counted (or, when the
//! walk must still expand them, read as bare next-node ids), and only the
//! `limit` rows of the page are materialized.

use std::collections::HashSet;

use mct_core::MAX_QUERY_DEPTH;

use crate::queries::{self, RelationHit, RelationPage, RelationSet, Resolution, ResolvedScope};
use crate::{Index, Result};

/// Breadth-first walk returning hits `offset..offset + limit` of the full
/// walk, in order: `first` are the relation sets of hop 1, `start` the symbol
/// ids the walk starts from, `hop` the single-hop set from one symbol id, and
/// `forward` picks which node a proven hit leads to (the resolved target, or
/// the source of a relation resolved to the node). `depth` is clamped to
/// `[1, MAX_QUERY_DEPTH]`; a visited-set of symbol ids keeps a cyclic graph
/// (e.g. mutual recursion) terminating. `total` is the hits skipped plus the
/// hits returned — exact once the walk runs dry, `offset + limit` otherwise.
#[allow(clippy::too_many_arguments)]
fn walk<'a>(
    index: &Index,
    first: Vec<RelationSet<'a>>,
    start: &[i64],
    depth: u32,
    limit: usize,
    offset: usize,
    forward: bool,
    hop: impl Fn(i64) -> RelationSet<'static>,
) -> Result<RelationPage> {
    let conn = &index.conn;
    let depth = depth.clamp(1, MAX_QUERY_DEPTH);
    let mut visited: HashSet<i64> = start.iter().copied().collect();
    let mut skip = offset;
    let mut skipped = 0usize;
    let mut hits = Vec::new();
    let mut sets = first;

    for level in 1..=depth {
        let expand = level < depth;
        let mut frontier = Vec::new();
        for set in sets {
            let mut from = 0;
            if skip > 0 {
                // Up to `skip` rows before the page are only counted — or,
                // when a later hop still needs them, read as next-node ids.
                from = if expand {
                    let nodes = queries::relation_next_nodes(conn, &set, forward, skip)?;
                    for &next in nodes.iter().flatten() {
                        if visited.insert(next) {
                            frontier.push(next);
                        }
                    }
                    nodes.len()
                } else {
                    queries::count_relations(conn, &set, Some(skip))?
                };
                skip -= from;
                skipped += from;
                if skip > 0 {
                    // The set ran dry before the page starts.
                    continue;
                }
            }
            // `hits.len() < limit` here: the walk returns as soon as it fills.
            let room = limit - hits.len();
            for mut hit in queries::query_relations_page(conn, &set, Some(room), from)? {
                hit.depth = level;
                let next = if forward {
                    hit.target_id
                } else {
                    proven_referrer(&hit)
                };
                if let Some(next) = next {
                    if visited.insert(next) {
                        frontier.push(next);
                    }
                }
                hits.push(hit);
            }
            if hits.len() >= limit {
                return Ok(RelationPage {
                    total: skipped + hits.len(),
                    hits,
                });
            }
        }
        if !expand || frontier.is_empty() {
            break;
        }
        sets = frontier.into_iter().map(&hop).collect();
    }
    Ok(RelationPage {
        total: skipped + hits.len(),
        hits,
    })
}

/// [`walk`] from one name-based hop-1 set. A single hop (`depth <= 1`)
/// reports its exact total, as the plain single-hop query always has, with
/// one `COUNT(*)` over the same set instead of reading every row.
#[allow(clippy::too_many_arguments)]
fn walk_from_name(
    index: &Index,
    first: RelationSet<'_>,
    start: &[i64],
    depth: u32,
    limit: usize,
    offset: usize,
    forward: bool,
    hop: impl Fn(i64) -> RelationSet<'static>,
) -> Result<RelationPage> {
    let mut page = walk(
        index,
        vec![first],
        start,
        depth,
        limit,
        offset,
        forward,
        hop,
    )?;
    // A short page means the walk ran dry, so its total is already exact.
    if depth <= 1 && page.hits.len() >= limit {
        page.total = queries::count_relations(&index.conn, &first, None)?;
    }
    Ok(page)
}

/// The definitions named `name` a backward walk starts from: those inside
/// `scope`, or — when the scope holds none, so it narrows only the referring
/// sites — every one. `true` alongside when that is only some of them.
fn scoped_start(index: &Index, name: &str, scope: ResolvedScope<'_>) -> Result<(Vec<i64>, bool)> {
    let all = queries::symbol_ids_named(&index.conn, name, ResolvedScope::default())?;
    let scoped = queries::symbol_ids_named(&index.conn, name, scope)?;
    if scoped.is_empty() {
        return Ok((all, false));
    }
    let partial = scoped.len() < all.len();
    Ok((scoped, partial))
}

/// A backward hop continues from the referring symbol only when its relation
/// resolved to the node being walked.
fn proven_referrer(hit: &RelationHit) -> Option<i64> {
    (hit.resolution == Resolution::Resolved).then_some(hit.from_symbol_id)
}

pub(crate) fn find_calls_bfs(
    index: &Index,
    function: &str,
    depth: u32,
    limit: usize,
    offset: usize,
    scope: ResolvedScope<'_>,
) -> Result<RelationPage> {
    let start = queries::symbol_ids_named(&index.conn, function, scope)?;
    // `scope` narrows the start definitions and hop 1 only; later hops
    // follow exact symbol ids across files.
    walk_from_name(
        index,
        queries::calls_named(function, scope),
        &start,
        depth,
        limit,
        offset,
        true,
        queries::calls_from_symbol,
    )
}

/// Backward walk from the definitions named `name`: callers only when
/// `calls_only`, every relation kind otherwise. Hop-1 hits that provably
/// target a definition other than the (scoped) start are left out — a scoped
/// start (`A::run` under a path) would otherwise report relations to a
/// same-named `B::run` outside it.
pub(crate) fn find_referrers_bfs(
    index: &Index,
    name: &str,
    calls_only: bool,
    depth: u32,
    limit: usize,
    offset: usize,
    scope: ResolvedScope<'_>,
) -> Result<RelationPage> {
    let (start, partial) = scoped_start(index, name, scope)?;
    // Every candidate of a relation shares its name, so a start holding every
    // definition named `name` drops nothing: skip the per-row filter then.
    let start_ids = partial.then(|| queries::id_list(&start));
    walk_from_name(
        index,
        queries::relations_to_start(name, calls_only, start_ids.as_deref(), scope),
        &start,
        depth,
        limit,
        offset,
        false,
        |id| queries::relations_reaching_symbol(id, calls_only),
    )
}

/// [`find_calls_bfs`] from exact symbol rows instead of a name: hop 1 is the
/// calls made by `start` only, never by another same-named definition.
pub(crate) fn find_calls_from(
    index: &Index,
    start: &[i64],
    depth: u32,
    limit: usize,
) -> Result<Vec<RelationHit>> {
    let first = start.iter().map(|&id| queries::calls_from_symbol(id));
    Ok(walk(
        index,
        first.collect(),
        start,
        depth,
        limit,
        0,
        true,
        queries::calls_from_symbol,
    )?
    .hits)
}

/// Relations that may reach the exact symbol rows `start` (resolved to one,
/// or ambiguous with one among the candidates), walked backward from proven
/// referrers only. `calls_only` keeps just `calls`.
pub(crate) fn find_referrers_of(
    index: &Index,
    start: &[i64],
    calls_only: bool,
    depth: u32,
    limit: usize,
) -> Result<Vec<RelationHit>> {
    let hop = |id| queries::relations_reaching_symbol(id, calls_only);
    let first = start.iter().map(|&id| hop(id));
    Ok(walk(index, first.collect(), start, depth, limit, 0, false, hop)?.hits)
}
