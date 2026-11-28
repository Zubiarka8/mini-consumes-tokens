//! The shortest path between two symbols (issue #633): a breadth-first
//! walk over outgoing calls (or calls and references) that records each
//! symbol's predecessor and stops at the first target reached.
//!
//! Only a [`Resolution::Resolved`] call proves an edge, so the first walk
//! follows those alone and its path is confirmed. When it finds none, a
//! second walk may also step through an ambiguous call to any of its
//! candidates; that path is only possible, and each such step says so. An
//! unresolved or external call is never followed.

use std::collections::{HashMap, HashSet};

use crate::queries::{self, RelationHit, Resolution, SymbolHit};
use crate::{Index, Result};

/// Most symbols one walk visits before it gives up, so a query over a dense
/// graph stays bounded.
pub const MAX_PATH_SYMBOLS: usize = 20_000;

/// One call of a path: the relation and the definition it steps to.
#[derive(Debug, Clone)]
pub struct PathStep {
    pub hit: RelationHit,
    pub target: SymbolHit,
}

/// What [`find_path`] found.
#[derive(Debug, Default)]
pub struct PathSearch {
    /// The calls from a start symbol to a target, in order; empty when no
    /// path was found, or when a start symbol is itself a target.
    pub steps: Vec<PathStep>,
    /// A path was found (possibly of no steps).
    pub found: bool,
    /// Every step resolved to its target. False when a step went through an
    /// ambiguous call.
    pub confirmed: bool,
    /// Symbols the last walk visited.
    pub searched: usize,
    /// The walk stopped at [`MAX_PATH_SYMBOLS`] before exhausting the
    /// graph within the hop limit.
    pub truncated: bool,
}

/// The shortest path from any of `from` to any of `to`, at most `max_hops`
/// relations long, over calls only or calls and references: a confirmed
/// one when one exists, else a possible one through ambiguous relations.
pub(crate) fn find_path(
    index: &Index,
    from: &[i64],
    to: &[i64],
    max_hops: u32,
    calls_only: bool,
) -> Result<PathSearch> {
    let targets: HashSet<i64> = to.iter().copied().collect();
    if from.iter().any(|id| targets.contains(id)) {
        return Ok(PathSearch {
            found: true,
            confirmed: true,
            ..PathSearch::default()
        });
    }
    let confirmed = walk(index, from, &targets, (max_hops, calls_only), false)?;
    if confirmed.found {
        return Ok(confirmed);
    }
    let possible = walk(index, from, &targets, (max_hops, calls_only), true)?;
    Ok(PathSearch {
        truncated: possible.truncated || confirmed.truncated,
        ..possible
    })
}

/// One breadth-first walk; `through_ambiguous` also steps from an ambiguous
/// call to each of its candidates.
fn walk(
    index: &Index,
    from: &[i64],
    targets: &HashSet<i64>,
    (max_hops, calls_only): (u32, bool),
    through_ambiguous: bool,
) -> Result<PathSearch> {
    // Each reached symbol: the symbol it was reached from and the call.
    let mut predecessor: HashMap<i64, (i64, RelationHit)> = HashMap::new();
    let mut seen: HashSet<i64> = from.iter().copied().collect();
    let mut frontier = from.to_vec();
    for hop in 1..=max_hops {
        let mut next = Vec::new();
        for &node in &frontier {
            for mut hit in queries::relations_from_symbol(&index.conn, node, calls_only)? {
                hit.depth = hop;
                let reached: Vec<i64> = match hit.resolution {
                    Resolution::Resolved => hit.target_id.into_iter().collect(),
                    Resolution::Ambiguous if through_ambiguous => {
                        queries::relation_candidates(&index.conn, hit.relation_id)?
                            .into_iter()
                            .map(|c| c.id)
                            .collect()
                    }
                    _ => Vec::new(),
                };
                for id in reached {
                    if !seen.insert(id) {
                        continue;
                    }
                    predecessor.insert(id, (node, hit.clone()));
                    if targets.contains(&id) {
                        let steps = steps_to(index, &predecessor, id)?;
                        return Ok(PathSearch {
                            confirmed: steps
                                .iter()
                                .all(|s| s.hit.resolution == Resolution::Resolved),
                            steps,
                            found: true,
                            searched: seen.len(),
                            truncated: false,
                        });
                    }
                    if seen.len() >= MAX_PATH_SYMBOLS {
                        return Ok(PathSearch {
                            searched: seen.len(),
                            truncated: true,
                            ..PathSearch::default()
                        });
                    }
                    next.push(id);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    Ok(PathSearch {
        searched: seen.len(),
        ..PathSearch::default()
    })
}

/// The calls leading to `end`, first to last, each with its target's row.
fn steps_to(
    index: &Index,
    predecessor: &HashMap<i64, (i64, RelationHit)>,
    end: i64,
) -> Result<Vec<PathStep>> {
    let mut chain = Vec::new();
    let mut at = end;
    while let Some((before, hit)) = predecessor.get(&at) {
        chain.push((at, hit.clone()));
        at = *before;
    }
    chain.reverse();
    let ids: Vec<i64> = chain.iter().map(|(id, _)| *id).collect();
    let rows: HashMap<i64, SymbolHit> = queries::symbols_by_ids(&index.conn, &ids)?
        .into_iter()
        .map(|row| (row.id, row))
        .collect();
    Ok(chain
        .into_iter()
        .filter_map(|(id, hit)| {
            Some(PathStep {
                hit,
                target: rows.get(&id)?.clone(),
            })
        })
        .collect())
}
