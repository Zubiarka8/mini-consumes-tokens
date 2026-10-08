//! Multi-hop BFS over the relation graph, layered on top of `Index`'s
//! existing single-hop query methods rather than a recursive SQL query.
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

use std::collections::HashSet;

use mct_core::MAX_QUERY_DEPTH;

use crate::queries::{self, RelationHit, Resolution, ResolvedScope};
use crate::{Index, Result};

/// The single-hop lookup from one symbol id: at most the given number of hits
/// (`None` = all), in the usual order.
type Fetch<'a> = dyn Fn(&Index, i64, Option<usize>) -> Result<Vec<RelationHit>> + 'a;

/// `Some(n)` when a walk is bounded to `n` hits, `None` for the unbounded
/// single-hop case (`usize::MAX`, see `bfs_budget`).
fn cap(budget: usize) -> Option<usize> {
    (budget != usize::MAX).then_some(budget)
}

/// Breadth-first walk: `first` is hop 1 (at least its first `budget` hits, in
/// order), `start` the symbol ids it starts from, `fetch` the single-hop
/// lookup from one symbol id and `next_node` the id a hit leads to, if it is
/// proven. `depth` is clamped to `[1, MAX_QUERY_DEPTH]`; a visited-set of
/// symbol ids keeps a cyclic graph (e.g. mutual recursion) terminating.
/// Stops once `budget` hits are collected, and never asks `fetch` for more
/// hits than the budget still has room for: hits past it would be dropped, so
/// the result is the same prefix of the full walk, for far less work.
fn bfs(
    index: &Index,
    first: Vec<RelationHit>,
    start: Vec<i64>,
    depth: u32,
    budget: usize,
    fetch: &Fetch<'_>,
    next_node: impl Fn(&RelationHit) -> Option<i64>,
) -> Result<Vec<RelationHit>> {
    if budget == 0 {
        return Ok(Vec::new());
    }
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
        // `results.len() < budget` here, so the room is positive.
        let room = cap(budget).map(|b| b - results.len());
        level_hits = Vec::new();
        for node in frontier {
            let want = room.map(|r| r - level_hits.len());
            level_hits.extend(fetch(index, node, want)?);
            if room.is_some_and(|r| level_hits.len() >= r) {
                break;
            }
        }
    }
    Ok(results)
}

/// First page size of [`first_hop_filtered`] when the budget is small.
const MIN_FIRST_HOP_PAGE: usize = 64;

/// Hop 1 of a name-based walk, bounded: the first `budget` hits of
/// `fetch_page(limit, offset)` that pass `keep`, in order. `keep` may drop
/// hits, so pages (doubling in size) are read until enough survive or the
/// source runs dry. Unbounded (`usize::MAX`), it is the one full query.
fn first_hop_filtered(
    budget: usize,
    fetch_page: impl Fn(usize, usize) -> Result<Vec<RelationHit>>,
    mut keep: impl FnMut(&RelationHit) -> Result<bool>,
) -> Result<Vec<RelationHit>> {
    let mut kept = Vec::new();
    let mut offset = 0usize;
    let mut page = budget.max(MIN_FIRST_HOP_PAGE);
    loop {
        let rows = fetch_page(page, offset)?;
        let exhausted = rows.len() < page;
        offset = offset.saturating_add(rows.len());
        for hit in rows {
            if keep(&hit)? {
                kept.push(hit);
                if kept.len() >= budget {
                    return Ok(kept);
                }
            }
        }
        if exhausted {
            return Ok(kept);
        }
        page = page.saturating_mul(2);
    }
}

/// The definitions named `name` a backward walk starts from: those inside
/// `scope`, or — when the scope holds none, so it narrows only the referring
/// sites — every one.
pub(crate) fn scoped_start(
    index: &Index,
    name: &str,
    scope: ResolvedScope<'_>,
) -> Result<Vec<i64>> {
    let start = queries::symbol_ids_named(&index.conn, name, scope)?;
    if start.is_empty() {
        return queries::symbol_ids_named(&index.conn, name, ResolvedScope::default());
    }
    Ok(start)
}

/// Drops hop-1 hits that provably target a definition other than `start`:
/// one resolved elsewhere, or ambiguous with no candidate in `start`. A
/// scoped start (`A::run` under a path) would otherwise report relations to
/// a same-named `B::run` outside it. Unresolved and external hits stay —
/// nothing proves they miss `start`.
fn reaches_start(index: &Index, hit: &RelationHit, start: &[i64]) -> Result<bool> {
    Ok(match hit.resolution {
        Resolution::Resolved => hit.target_id.is_some_and(|id| start.contains(&id)),
        Resolution::Ambiguous => queries::relation_candidates(&index.conn, hit.relation_id)?
            .iter()
            .any(|c| start.contains(&c.id)),
        Resolution::External | Resolution::Unresolved => true,
    })
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
    if budget == 0 {
        return Ok(Vec::new());
    }
    let start = queries::symbol_ids_named(&index.conn, function, scope)?;
    let first = first_hop_filtered(
        budget,
        |limit, offset| queries::find_calls_page(&index.conn, function, scope, limit, offset),
        |_| Ok(true),
    )?;
    bfs(
        index,
        first,
        start,
        depth,
        budget,
        // `scope` narrows the start definitions and hop 1 only; later hops
        // follow exact symbol ids across files.
        &|idx: &Index, id, limit| {
            queries::calls_from_symbol(&idx.conn, id, ResolvedScope::default(), limit)
        },
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
    if budget == 0 {
        return Ok(Vec::new());
    }
    let start = scoped_start(index, function, scope)?;
    let first = first_hop_filtered(
        budget,
        |limit, offset| queries::find_callers_page(&index.conn, function, scope, limit, offset),
        |hit| reaches_start(index, hit, &start),
    )?;
    bfs(
        index,
        first,
        start,
        depth,
        budget,
        &|idx: &Index, id, limit| {
            queries::relations_reaching_symbol(&idx.conn, id, true, ResolvedScope::default(), limit)
        },
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
    if budget == 0 {
        return Ok(Vec::new());
    }
    let start = scoped_start(index, symbol, scope)?;
    let first = first_hop_filtered(
        budget,
        |limit, offset| queries::find_references_page(&index.conn, symbol, scope, limit, offset),
        |hit| reaches_start(index, hit, &start),
    )?;
    bfs(
        index,
        first,
        start,
        depth,
        budget,
        &|idx: &Index, id, limit| {
            queries::relations_reaching_symbol(
                &idx.conn,
                id,
                false,
                ResolvedScope::default(),
                limit,
            )
        },
        proven_referrer,
    )
}

/// Hop 1 of an identity-based walk: `fetch` from every start id, in order,
/// until `budget` hits are in hand.
fn first_hop(
    index: &Index,
    start: &[i64],
    budget: usize,
    fetch: &Fetch<'_>,
) -> Result<Vec<RelationHit>> {
    let mut hits = Vec::new();
    for &id in start {
        let want = cap(budget).map(|b| b - hits.len());
        hits.extend(fetch(index, id, want)?);
        if hits.len() >= budget {
            break;
        }
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
    if budget == 0 {
        return Ok(Vec::new());
    }
    let fetch = |idx: &Index, id, limit| {
        queries::calls_from_symbol(&idx.conn, id, ResolvedScope::default(), limit)
    };
    let first = first_hop(index, start, budget, &fetch)?;
    bfs(index, first, start.to_vec(), depth, budget, &fetch, |hit| {
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
    if budget == 0 {
        return Ok(Vec::new());
    }
    let fetch = |idx: &Index, id, limit| {
        queries::relations_reaching_symbol(
            &idx.conn,
            id,
            calls_only,
            ResolvedScope::default(),
            limit,
        )
    };
    let first = first_hop(index, start, budget, &fetch)?;
    bfs(
        index,
        first,
        start.to_vec(),
        depth,
        budget,
        &fetch,
        proven_referrer,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod bounded_walk_tests {
    use std::cell::RefCell;

    use super::*;
    use crate::ExcludeSet;

    fn hit(target: i64) -> RelationHit {
        RelationHit {
            resolution: Resolution::Resolved,
            target_id: Some(target),
            ..RelationHit::default()
        }
    }

    /// A graph where every node has `fan_out` distinct, never-repeating
    /// successors; records the limit each lookup was given.
    fn walk(depth: u32, budget: usize, fan_out: i64) -> (Vec<RelationHit>, Vec<Option<usize>>) {
        let index = Index::open_in_memory(&std::env::temp_dir(), ExcludeSet::default()).unwrap();
        let asked = RefCell::new(Vec::new());
        let fetch = |_: &Index, id: i64, limit: Option<usize>| {
            asked.borrow_mut().push(limit);
            let n = limit.map_or(fan_out, |l| fan_out.min(l as i64));
            Ok((0..n).map(|k| hit(id * 1000 + k + 1)).collect())
        };
        let first = (0..fan_out).map(|k| hit(k + 1)).collect::<Vec<_>>();
        let hits = bfs(
            &index,
            first,
            vec![0],
            depth,
            budget,
            &fetch,
            |h: &RelationHit| h.target_id,
        )
        .unwrap();
        (hits, asked.into_inner())
    }

    #[test]
    fn a_lookup_is_never_asked_for_more_than_the_budget_has_room_for() {
        let (hits, asked) = walk(5, 25, 10);
        assert_eq!(hits.len(), 25);
        // 10 hits at hop 1 leave room for 15; the first node alone supplies
        // 10, the second the last 5, and the walk stops there.
        assert_eq!(asked, [Some(15), Some(5)]);
    }

    #[test]
    fn an_unbounded_walk_asks_for_everything() {
        let (hits, asked) = walk(2, usize::MAX, 4);
        assert_eq!(hits.len(), 4 + 4 * 4);
        assert!(asked.iter().all(Option::is_none));
    }

    #[test]
    fn a_budget_filled_at_hop_one_fetches_nothing_further() {
        let (hits, asked) = walk(5, 10, 10);
        assert_eq!(hits.len(), 10);
        assert!(asked.is_empty());
    }

    #[test]
    fn a_zero_budget_returns_nothing_and_fetches_nothing() {
        let (hits, asked) = walk(5, 0, 10);
        assert!(hits.is_empty());
        assert!(asked.is_empty());
    }

    #[test]
    fn a_cycle_is_reported_back_to_its_start_but_never_expanded_twice() {
        let index = Index::open_in_memory(&std::env::temp_dir(), ExcludeSet::default()).unwrap();
        // 1 -> 2 -> 1 (the start): every node's only successor is the other.
        let fetch = |_: &Index, id: i64, _: Option<usize>| Ok(vec![hit(3 - id)]);
        let hits = bfs(
            &index,
            vec![hit(1)],
            vec![0],
            32,
            100,
            &fetch,
            |h: &RelationHit| h.target_id,
        )
        .unwrap();
        // The back-edge 2 -> 1 is a real relation, so it is reported once at
        // depth 3; node 1 is not expanded again, so the walk ends there.
        assert_eq!(hits.iter().map(|h| h.depth).collect::<Vec<_>>(), [1, 2, 3]);
        assert_eq!(hits[2].target_id, Some(1));
    }
}
