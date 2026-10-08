//! Relation-query pagination: bounded retrieval, ordering and the scale
//! measurement behind the budgets (issue R02).
//!
//! The ordinary tests are deterministic and free of timing assertions. The
//! measurement is `#[ignore]`d; run it on demand with
//!
//! ```text
//! MCT_SCALE_SYMBOLS=10000,100000 cargo test --locked -p mct-index \
//!     --release --test relation_scale -- --ignored --nocapture
//! ```
//!
//! and wrap it in `/usr/bin/time -l` (macOS) or `/usr/bin/time -v` (Linux) for
//! peak resident memory. On Linux the test also prints `VmHWM` itself.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use mct_core::{
    Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolKind, SymbolRecord,
    SymbolRelation,
};
use mct_index::{ExcludeSet, Index, RelationHit, Resolution};

/// `fn NAME [calls CALLEE...]` per line; every callee is a separate `calls`
/// relation on that same line (distinct columns), which is what makes
/// same-path/same-line ties constructible.
struct FakeParser;

impl mct_core::LanguageParser for FakeParser {
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
                level: None,
            });
            if parts.next() == Some("calls") {
                for (n, callee) in parts.enumerate() {
                    parsed.relations.push(SymbolRelation {
                        from: id,
                        kind: RelationKind::Calls,
                        to_name: callee.to_string(),
                        location: Location {
                            line: line_no as u32 + 1,
                            column: n as u32 + 1,
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

fn registry() -> mct_core::LanguageRegistry {
    let mut registry = mct_core::LanguageRegistry::new();
    registry.register(Arc::new(FakeParser));
    registry
}

fn tempdir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let id = nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed));
    let dir = std::env::temp_dir().join(format!("mct-index-relation-scale-{id}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn index_of(files: &[(&str, String)]) -> Index {
    let dir = tempdir();
    for (name, contents) in files {
        fs::write(dir.join(name), contents).unwrap();
    }
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    index
}

/// `symbols` functions, all calling `hot`, spread over files of 1000 lines.
/// `hot` is defined once (every call resolves) or, with `ambiguous`, in two
/// files (every call is ambiguous with two candidates).
fn fan_in_files(symbols: usize, ambiguous: bool) -> Vec<(String, String)> {
    let mut files = Vec::new();
    let per_file = 1000;
    for (f, start) in (0..symbols).step_by(per_file).enumerate() {
        let mut body = String::new();
        for i in start..(start + per_file).min(symbols) {
            body.push_str(&format!("fn c{i} calls hot\n"));
        }
        files.push((format!("f{f:05}.fake"), body));
    }
    files.push(("hot.fake".to_string(), "fn hot\n".to_string()));
    if ambiguous {
        files.push(("hot2.fake".to_string(), "fn hot\n".to_string()));
    }
    files
}

fn fan_in_index(symbols: usize, ambiguous: bool) -> Index {
    let files = fan_in_files(symbols, ambiguous);
    let borrowed: Vec<(&str, String)> =
        files.iter().map(|(n, c)| (n.as_str(), c.clone())).collect();
    index_of(&borrowed)
}

fn key(hit: &RelationHit) -> (String, u32, u32, i64) {
    (
        hit.relative_path.clone(),
        hit.line,
        hit.column,
        hit.relation_id,
    )
}

/// Pages of `limit` hits at `offset = 0, limit, 2*limit, ..` must concatenate
/// to the same list a single large page returns.
fn assert_paging_equivalent(
    fetch: impl Fn(usize, usize) -> Vec<RelationHit>,
    total: usize,
    limit: usize,
) {
    let whole = fetch(total + 5, 0);
    assert_eq!(whole.len(), total);
    let mut paged = Vec::new();
    let mut offset = 0;
    while offset < total {
        let page = fetch(limit, offset);
        // The BFS API returns the first `limit + offset` hits; the caller skips
        // `offset` of them (see the server's pagination).
        paged.extend(page.into_iter().skip(offset));
        offset += limit;
    }
    assert_eq!(
        whole.iter().map(key).collect::<Vec<_>>(),
        paged.iter().map(key).collect::<Vec<_>>()
    );
}

#[test]
fn multi_hop_pages_concatenate_to_the_full_walk() {
    let index = fan_in_index(250, false);
    let limit = 40;
    assert_paging_equivalent(
        |l, o| index.find_callers_bfs("hot", 3, l, o).unwrap(),
        250,
        limit,
    );
    assert_paging_equivalent(
        |l, o| index.find_references_bfs("hot", 3, l, o).unwrap(),
        250,
        limit,
    );
}

#[test]
fn a_zero_budget_walk_returns_nothing() {
    let index = fan_in_index(30, false);
    assert!(index.find_callers_bfs("hot", 3, 0, 0).unwrap().is_empty());
    assert!(index
        .find_references_bfs("hot", 3, 0, 0)
        .unwrap()
        .is_empty());
    assert!(index.find_calls_bfs("c0", 3, 0, 0).unwrap().is_empty());
}

#[test]
fn a_bounded_walk_is_a_prefix_of_the_unbounded_one() {
    let index = fan_in_index(120, true);
    let full = index.find_callers_bfs("hot", 3, 1000, 0).unwrap();
    assert_eq!(full.len(), 120);
    for budget in [1usize, 2, 7, 64, 119, 120] {
        let part = index.find_callers_bfs("hot", 3, budget, 0).unwrap();
        assert_eq!(
            part.iter().map(key).collect::<Vec<_>>(),
            full.iter().take(budget).map(key).collect::<Vec<_>>(),
            "budget {budget}"
        );
    }
}

#[test]
fn bounded_hits_keep_ambiguity_and_candidate_provenance() {
    let index = fan_in_index(60, true);
    let full = index.find_callers_bfs("hot", 2, 1000, 0).unwrap();
    let part = index.find_callers_bfs("hot", 2, 10, 0).unwrap();
    assert_eq!(part.len(), 10);
    for (p, f) in part.iter().zip(&full) {
        assert_eq!(p.resolution, Resolution::Ambiguous);
        assert_eq!(p.candidate_count, 2);
        assert_eq!(p.candidates.len(), f.candidates.len());
        assert_eq!(
            p.candidates
                .iter()
                .map(|c| (c.relative_path.clone(), c.line))
                .collect::<Vec<_>>(),
            f.candidates
                .iter()
                .map(|c| (c.relative_path.clone(), c.line))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn same_path_and_line_ties_order_by_column_then_relation_id() {
    // One line making four calls to the same target: only the column tells
    // them apart, so a LIMIT without a total order would be non-deterministic.
    let index = index_of(&[
        ("a.fake", "fn a calls t t t t\n".to_string()),
        ("t.fake", "fn t\n".to_string()),
    ]);
    let full = index.find_callers_bfs("t", 2, 100, 0).unwrap();
    assert_eq!(
        full.iter().map(|h| h.column).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    for budget in 1..=4usize {
        let part = index.find_callers_bfs("t", 2, budget, 0).unwrap();
        assert_eq!(
            part.iter().map(|h| h.column).collect::<Vec<_>>(),
            (1..=budget as u32).collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_long_chain_with_a_cycle_respects_depth_and_budget() {
    // c0 -> c1 -> ... -> c9 -> c0, plus fan-out at every node.
    let mut body = String::new();
    for i in 0..10 {
        body.push_str(&format!("fn c{i} calls c{} x{i}\n", (i + 1) % 10));
    }
    let index = index_of(&[("chain.fake", body)]);
    let all = index.find_calls_bfs("c0", 32, 1000, 0).unwrap();
    // Every call of every reachable node is reported once; the cycle closes.
    assert_eq!(all.len(), 20);
    let hops: Vec<u32> = all.iter().map(|h| h.depth).collect();
    assert!(hops.windows(2).all(|w| w[0] <= w[1]), "{hops:?}");
    for budget in [1usize, 5, 13, 19] {
        let part = index.find_calls_bfs("c0", 32, budget, 0).unwrap();
        assert_eq!(
            part.iter().map(key).collect::<Vec<_>>(),
            all.iter().take(budget).map(key).collect::<Vec<_>>()
        );
    }
}

#[test]
fn depth_one_still_reports_the_uncapped_direct_total() {
    let index = fan_in_index(200, false);
    // `limit`/`offset` never shrink a single-hop walk: the caller derives its
    // reported total from the length.
    assert_eq!(index.find_callers_bfs("hot", 1, 5, 0).unwrap().len(), 200);
}

fn peak_rss_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.parse().ok())
}

/// Not a pass/fail timing test: prints elapsed time per scenario so the
/// budgets in the module documentation can be (re)checked on any machine.
#[test]
#[ignore = "not a bug: measurement; run with --ignored --nocapture"]
fn relation_scale_measurement() {
    let scales: Vec<usize> = std::env::var("MCT_SCALE_SYMBOLS")
        .unwrap_or_else(|_| "10000,100000".to_string())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    for symbols in scales {
        for ambiguous in [false, true] {
            let index = fan_in_index(symbols, ambiguous);
            let kind = if ambiguous { "ambiguous" } else { "resolved" };
            let scenarios: [(&str, u32, usize); 3] = [
                ("depth1 uncapped", 1, 50),
                ("depth3 limit 50", 3, 50),
                ("depth3 limit 50 offset 50 (budget 100)", 3, 100),
            ];
            for (label, depth, budget) in scenarios {
                let started = Instant::now();
                let hits = index.find_callers_bfs("hot", depth, budget, 0).unwrap();
                println!(
                    "symbols={symbols} {kind} {label}: {} hits in {:?} (peak RSS KiB: {:?})",
                    hits.len(),
                    started.elapsed(),
                    peak_rss_kib()
                );
            }
        }
    }
}

#[test]
fn direct_tool_pages_only_materialize_the_requested_rows() {
    use mct_index::{QueryScope, RelationDirection};
    let index = fan_in_index(10_000, false);
    for direction in [RelationDirection::Callers, RelationDirection::References] {
        let page = index
            .relation_page("hot", direction, 1, (1, 0), QueryScope::default())
            .unwrap();
        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.total, Some(10_000));
        assert!(page.has_more);
        let last = index
            .relation_page("hot", direction, 0, (1, 9_999), QueryScope::default())
            .unwrap();
        assert_eq!(last.hits.len(), 1);
        assert!(!last.has_more);
        let empty = index
            .relation_page("hot", direction, 1, (0, usize::MAX), QueryScope::default())
            .unwrap();
        assert!(empty.hits.is_empty());
        assert_eq!(empty.total, Some(10_000));
    }
}

#[test]
fn multihop_pages_have_truthful_continuation_and_no_gaps() {
    use mct_index::{QueryScope, RelationDirection};
    let index = index_of(&[(
        "a.fake",
        "fn a calls b c d\nfn b calls e\nfn c\nfn d\nfn e\n".into(),
    )]);
    let full = index
        .relation_page(
            "a",
            RelationDirection::Calls,
            2,
            (10, 0),
            QueryScope::default(),
        )
        .unwrap();
    assert_eq!(full.total, Some(4));
    for offset in 0..4 {
        let page = index
            .relation_page(
                "a",
                RelationDirection::Calls,
                2,
                (1, offset),
                QueryScope::default(),
            )
            .unwrap();
        assert_eq!(page.hits[0].relation_id, full.hits[offset].relation_id);
        assert_eq!(page.has_more, offset < 3);
        assert_eq!(page.total, if offset < 3 { None } else { Some(4) });
    }
    let empty = index
        .relation_page(
            "a",
            RelationDirection::Calls,
            2,
            (0, 0),
            QueryScope::default(),
        )
        .unwrap();
    assert!(empty.hits.is_empty());
    assert!(empty.has_more);
    let past = index
        .relation_page(
            "a",
            RelationDirection::Calls,
            2,
            (1, usize::MAX),
            QueryScope::default(),
        )
        .unwrap();
    assert!(past.hits.is_empty());
    assert_eq!(past.total, Some(4));
}
