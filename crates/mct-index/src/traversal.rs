//! Multi-hop BFS over the relation graph, layered on top of `Index`'s
//! existing single-hop query methods rather than a recursive SQL query.
//! This keeps the SQL in `queries.rs` simple and makes cycle-safety (a
//! visited-set of symbol ids) trivial to reason about and test, at the
//! cost of one query per node instead of one recursive query — an
//! acceptable trade for the repo sizes this project targets.
//!
//! Hop 1 is the name-based query, unchanged: every relation spelled (or made
//! by a symbol named) the start name, whatever its resolution. Every later
//! hop walks symbol ids, and only along edges whose target is proven: a
//! forward hop follows a [`Resolution::Resolved`] callee, a backward hop
//! continues only from a caller whose relation resolved to the node. An
//! ambiguous or unresolved relation is reported where it is found, but never
//! expanded, so a walk can't wander into a same-named unrelated definition.

use std::collections::HashSet;

use mct_core::MAX_QUERY_DEPTH;

use crate::queries::{self, RelationHit, Resolution, ResolvedScope};
use crate::{Index, Result};

/// Breadth-first walk: `first` is hop 1, `start` the symbol ids it starts
/// from, `fetch` the single-hop lookup from one symbol id and `next_node`
/// the id a hit leads to, if it is proven. `depth` is clamped to
/// `[1, MAX_QUERY_DEPTH]`; a visited-set of symbol ids keeps a cyclic graph
/// (e.g. mutual recursion) terminating. Stops once `budget` hits are
/// collected.
fn bfs(
    index: &Index,
    first: Vec<RelationHit>,
    start: Vec<i64>,
    depth: u32,
    budget: usize,
    fetch: impl Fn(&Index, i64) -> Result<Vec<RelationHit>>,
    next_node: impl Fn(&RelationHit) -> Option<i64>,
) -> Result<Vec<RelationHit>> {
    let depth = depth.clamp(1, MAX_QUERY_DEPTH);
    let mut visited: HashSet<i64> = start.into_iter().collect();
    let mut results = Vec::new();
    let mut level_hits = first;

    for level in 1..=depth {
        let mut frontier = Vec::new();
        for mut hit in level_hits {
            hit.depth = level;
            if let Some(next) = next_node(&hit) {
                if visited.insert(next) {
                    frontier.push(next);
                }
            }
            results.push(hit);
            if results.len() >= budget {
                return Ok(results);
            }
        }
        if level == depth || frontier.is_empty() {
            break;
        }
        level_hits = Vec::new();
        for node in frontier {
            level_hits.extend(fetch(index, node)?);
        }
    }
    Ok(results)
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
    budget: usize,
    scope: ResolvedScope<'_>,
) -> Result<Vec<RelationHit>> {
    let start = queries::symbol_ids_named(&index.conn, function, scope)?;
    bfs(
        index,
        queries::find_calls_scoped(&index.conn, function, scope)?,
        start,
        depth,
        budget,
        // `scope` narrows the start definitions and hop 1 only; later hops
        // follow exact symbol ids across files.
        |idx, id| queries::calls_from_symbol(&idx.conn, id, ResolvedScope::default()),
        |hit| hit.target_id,
    )
}

pub(crate) fn find_callers_bfs(
    index: &Index,
    function: &str,
    depth: u32,
    budget: usize,
    scope: ResolvedScope<'_>,
) -> Result<Vec<RelationHit>> {
    let start = queries::symbol_ids_named(&index.conn, function, scope)?;
    bfs(
        index,
        queries::find_callers_scoped(&index.conn, function, scope)?,
        start,
        depth,
        budget,
        |idx, id| queries::relations_reaching_symbol(&idx.conn, id, true, ResolvedScope::default()),
        proven_referrer,
    )
}

pub(crate) fn find_references_bfs(
    index: &Index,
    symbol: &str,
    depth: u32,
    budget: usize,
    scope: ResolvedScope<'_>,
) -> Result<Vec<RelationHit>> {
    let start = queries::symbol_ids_named(&index.conn, symbol, scope)?;
    bfs(
        index,
        queries::find_references_scoped(&index.conn, symbol, scope)?,
        start,
        depth,
        budget,
        |idx, id| {
            queries::relations_reaching_symbol(&idx.conn, id, false, ResolvedScope::default())
        },
        proven_referrer,
    )
}

/// Hop 1 of an identity-based walk: `fetch` from every start id, in order.
fn first_hop(
    index: &Index,
    start: &[i64],
    fetch: &impl Fn(&Index, i64) -> Result<Vec<RelationHit>>,
) -> Result<Vec<RelationHit>> {
    let mut hits = Vec::new();
    for &id in start {
        hits.extend(fetch(index, id)?);
    }
    Ok(hits)
}

/// [`find_calls_bfs`] from exact symbol rows instead of a name: hop 1 is the
/// calls made by `start` only, never by another same-named definition.
pub(crate) fn find_calls_from(
    index: &Index,
    start: &[i64],
    depth: u32,
    budget: usize,
) -> Result<Vec<RelationHit>> {
    let fetch =
        |idx: &Index, id| queries::calls_from_symbol(&idx.conn, id, ResolvedScope::default());
    let first = first_hop(index, start, &fetch)?;
    bfs(index, first, start.to_vec(), depth, budget, fetch, |hit| {
        hit.target_id
    })
}

/// Relations that may reach the exact symbol rows `start` (resolved to one,
/// or ambiguous with one among the candidates), walked backward from proven
/// referrers only. `calls_only` keeps just `calls`.
pub(crate) fn find_referrers_of(
    index: &Index,
    start: &[i64],
    calls_only: bool,
    depth: u32,
    budget: usize,
) -> Result<Vec<RelationHit>> {
    let fetch = |idx: &Index, id| {
        queries::relations_reaching_symbol(&idx.conn, id, calls_only, ResolvedScope::default())
    };
    let first = first_hop(index, start, &fetch)?;
    bfs(
        index,
        first,
        start.to_vec(),
        depth,
        budget,
        fetch,
        proven_referrer,
    )
}
