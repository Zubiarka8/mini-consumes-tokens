//! Opt-in paired fixture measurement; run identically on baseline and candidate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::{
    reviews::{Kind, Status},
    store::Store,
};
use mct_index::{ExcludeSet, Index};
use std::time::Instant;

fn summarize(name: &str, mut values: Vec<u128>) {
    values.sort_unstable();
    println!(
        "{name}: samples={} p50_us={} p95_us={} max_us={}",
        values.len(),
        values[values.len() / 2],
        values[values.len() * 95 / 100],
        values[values.len() - 1]
    );
}

#[test]
#[ignore = "explicit paired performance fixture, excluded from correctness suite"]
fn coordination_fixture_benchmark() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    std::fs::create_dir(root.as_path().join(".mct-index")).unwrap();
    for n in 0..100 {
        std::fs::write(
            root.as_path().join(format!("source{n}.rs")),
            format!("pub fn measured_symbol_{n}() -> usize {{ {n} }}\n"),
        )
        .unwrap();
    }
    let path = root.as_path().join(".mct-index/index.sqlite3");
    let registry = mct_languages::build_registry();
    let mut index = Index::open(root.as_path(), &path, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let mut store = Store::open(root.as_path()).unwrap();
    let identity = "a".repeat(64);
    let fingerprint = "b".repeat(64);
    let (mut writes, mut indexing, mut queries) = (Vec::new(), Vec::new(), Vec::new());
    for n in 0..53 {
        let start = Instant::now();
        store
            .set(
                (Kind::Duplicate, &identity, &fingerprint),
                if n % 2 == 0 {
                    Status::Ignored
                } else {
                    Status::FalsePositive
                },
                "fixture",
                n,
            )
            .unwrap();
        let write = start.elapsed().as_micros();
        let start = Instant::now();
        index.reindex(&registry, false).unwrap();
        let update = start.elapsed().as_micros();
        let start = Instant::now();
        for _ in 0..100 {
            assert_eq!(index.find_symbol("measured_symbol_50").unwrap().len(), 1);
        }
        let query = start.elapsed().as_micros();
        if n >= 3 {
            writes.push(write);
            indexing.push(update);
            queries.push(query);
        }
    }
    summarize("dashboard_save", writes);
    summarize("unchanged_reindex_100_files", indexing);
    summarize("100_symbol_queries", queries);
}

#[test]
fn store_schema_and_ai_reader_cap_agree() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    std::fs::create_dir(root.as_path().join(".mct-index")).unwrap();
    let _store = Store::open(root.as_path()).unwrap();
    let conn =
        rusqlite::Connection::open(root.as_path().join(".mct-index/dashboard.sqlite3")).unwrap();
    assert_eq!(
        conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        mct_index::review_state::SUPPORTED_SCHEMA_VERSION
    );
}
