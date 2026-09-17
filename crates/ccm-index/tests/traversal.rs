//! Multi-hop BFS (`Index::find_calls_bfs`/`find_callers_bfs`/`find_references_bfs`)
//! correctness: cycle-safety on a cyclic call graph, the `MAX_QUERY_DEPTH`
//! ceiling on a long acyclic chain, and the `limit`/`offset` budget early-stop.
//! Uses the same toy `fn NAME calls OTHER`-per-line fake parser as `reindex.rs`,
//! duplicated locally since Rust integration test files don't share code.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/ccm-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::sync::Arc;

use ccm_core::{
    Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolKind, SymbolRecord,
    SymbolRelation,
};
use ccm_index::{ExcludeSet, Index};

struct FakeParser;

impl ccm_core::LanguageParser for FakeParser {
    fn language_id(&self) -> &'static str {
        "fake"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["fake"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parsed = ParsedFile::default();
        for (line_no, line) in file.contents.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            if parts.next() != Some("fn") {
                return Err(ParseError::Syntax {
                    path: file.relative_path.clone(),
                    line: line_no as u32 + 1,
                    message: format!("expected `fn`, got `{line}`"),
                });
            }
            let name = parts.next().unwrap_or_default().to_string();
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name,
                kind: SymbolKind::Function,
                location: Location {
                    line: line_no as u32 + 1,
                    column: 1,
                    byte_len: line.len() as u32,
                    end_line: Some(line_no as u32 + 1),
                },
                parent: None,
            });
            if parts.next() == Some("calls") {
                if let Some(callee) = parts.next() {
                    parsed.relations.push(SymbolRelation {
                        from: id,
                        kind: RelationKind::Calls,
                        to_name: callee.to_string(),
                        location: Location {
                            line: line_no as u32 + 1,
                            column: 1,
                            byte_len: line.len() as u32,
                            end_line: None,
                        },
                    });
                }
            }
        }
        Ok(parsed)
    }
}

fn registry() -> ccm_core::LanguageRegistry {
    let mut registry = ccm_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    registry
}

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ccm-index-traversal-test-{}", uuid_like()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn uuid_like() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// a -> b -> c -> a: a 3-node cycle, one call per function.
fn cyclic_index() -> Index {
    let dir = tempdir();
    fs::write(dir.join("a.fake"), "fn a calls b\n").unwrap();
    fs::write(dir.join("b.fake"), "fn b calls c\n").unwrap();
    fs::write(dir.join("c.fake"), "fn c calls a\n").unwrap();
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    index
}

#[test]
fn depth_1_bfs_matches_the_plain_single_hop_query_exactly() {
    let index = cyclic_index();
    let plain = index.find_calls("a").unwrap();
    let bfs = index.find_calls_bfs("a", 1, 50, 0).unwrap();
    assert_eq!(plain.len(), bfs.len());
    assert_eq!(plain[0].to_name, bfs[0].to_name);
    assert_eq!(bfs[0].depth, 1);
}

#[test]
fn bfs_walks_multiple_hops_and_tags_each_hit_with_its_hop_number() {
    let index = cyclic_index();
    let hits = index.find_calls_bfs("a", 2, 50, 0).unwrap();
    // a->b (depth 1), b->c (depth 2).
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert_eq!((hits[0].to_name.as_str(), hits[0].depth), ("b", 1));
    assert_eq!((hits[1].to_name.as_str(), hits[1].depth), ("c", 2));
}

#[test]
fn bfs_on_a_cycle_terminates_instead_of_looping_forever() {
    let index = cyclic_index();
    // Depth far exceeds the cycle length (3): a visited-set must stop
    // re-expanding `a` once the walk returns to it, or this test would hang.
    let hits = index.find_calls_bfs("a", 20, 1000, 0).unwrap();
    assert_eq!(hits.len(), 3, "{hits:?}");
    assert_eq!(hits[2].depth, 3);
    assert_eq!(hits[2].to_name, "a", "the cycle-closing edge c->a is still reported once");

    let callers = index.find_callers_bfs("a", 20, 1000, 0).unwrap();
    assert_eq!(callers.len(), 3, "{callers:?}");

    let refs = index.find_references_bfs("a", 20, 1000, 0).unwrap();
    assert_eq!(refs.len(), 3, "{refs:?}");
}

#[test]
fn requested_depth_beyond_max_query_depth_is_clamped() {
    let dir = tempdir();
    // A 40-node acyclic chain: a0 -> a1 -> ... -> a39, each file one hop.
    for i in 0..39u32 {
        fs::write(dir.join(format!("n{i}.fake")), format!("fn a{i} calls a{}\n", i + 1)).unwrap();
    }
    fs::write(dir.join("n39.fake"), "fn a39\n").unwrap();
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();

    // ccm_core::MAX_QUERY_DEPTH is 32; a depth request far beyond it must not
    // walk the full 39-hop chain.
    let hits = index.find_calls_bfs("a0", 1000, 10_000, 0).unwrap();
    assert_eq!(hits.len(), ccm_core::MAX_QUERY_DEPTH as usize, "{hits:?}");
    assert_eq!(hits.last().unwrap().depth, ccm_core::MAX_QUERY_DEPTH);
}

#[test]
fn limit_plus_offset_budget_stops_the_walk_early() {
    let index = cyclic_index();
    // budget = limit + offset = 1 + 1 = 2, so the walk must stop after the
    // second hit even though depth allows more.
    let hits = index.find_calls_bfs("a", 20, 1, 1).unwrap();
    assert_eq!(hits.len(), 2, "{hits:?}");
}
