//! Multi-hop BFS over the relation graph, layered on top of `Index`'s
//! existing single-hop query methods rather than a recursive SQL query.
//! This keeps the SQL in `queries.rs` simple and makes cycle-safety (a
//! visited-set of symbol names) trivial to reason about and test, at the
//! cost of one query per BFS level instead of one recursive query — an
//! acceptable trade for the repo sizes this project targets.

use std::collections::HashSet;

use mct_core::MAX_QUERY_DEPTH;

use crate::{queries::RelationHit, Index, Result};

/// Walks the relation graph breadth-first starting from `start`, using
/// `fetch` as the per-node single-hop lookup and `next_node` to pick, from
/// each hit found while expanding a node, which symbol name to explore next.
/// `depth` is clamped to `[1, MAX_QUERY_DEPTH]`; a visited-set keyed by
/// symbol name prevents revisiting a node, which is what keeps this
/// terminating on a cyclic call graph (e.g. mutual recursion) regardless of
/// `depth`. Stops early once `budget` hits have been collected.
fn bfs(
    index: &Index,
    start: &str,
    depth: u32,
    budget: usize,
    fetch: impl Fn(&Index, &str) -> Result<Vec<RelationHit>>,
    next_node: impl Fn(&RelationHit) -> &str,
) -> Result<Vec<RelationHit>> {
    let depth = depth.clamp(1, MAX_QUERY_DEPTH);
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(start.to_string());
    let mut frontier = vec![start.to_string()];
    let mut results = Vec::new();

    'levels: for level in 1..=depth {
        let mut next_frontier = Vec::new();
        for node in &frontier {
            for mut hit in fetch(index, node)? {
                hit.depth = level;
                let next = next_node(&hit).to_string();
                if visited.insert(next.clone()) {
                    next_frontier.push(next);
                }
                results.push(hit);
                if results.len() >= budget {
                    break 'levels;
                }
            }
        }
        if next_frontier.is_empty() {
            break;
        }
        frontier = next_frontier;
    }
    Ok(results)
}

pub(crate) fn find_calls_bfs(
    index: &Index,
    function: &str,
    depth: u32,
    budget: usize,
) -> Result<Vec<RelationHit>> {
    bfs(
        index,
        function,
        depth,
        budget,
        |idx, name| idx.find_calls(name),
        |hit| hit.to_name.as_str(),
    )
}

pub(crate) fn find_callers_bfs(
    index: &Index,
    function: &str,
    depth: u32,
    budget: usize,
) -> Result<Vec<RelationHit>> {
    bfs(
        index,
        function,
        depth,
        budget,
        |idx, name| idx.find_callers(name),
        |hit| hit.from_symbol.as_str(),
    )
}

pub(crate) fn find_references_bfs(
    index: &Index,
    symbol: &str,
    depth: u32,
    budget: usize,
) -> Result<Vec<RelationHit>> {
    bfs(
        index,
        symbol,
        depth,
        budget,
        |idx, name| idx.find_references(name),
        |hit| hit.from_symbol.as_str(),
    )
}
