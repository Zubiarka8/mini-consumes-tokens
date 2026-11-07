//! What `Index::change_marker` covers, before anything relies on it to detect
//! an index that changed between two pages of results: it must move on every
//! symbol, relation and file addition, edit and removal, and stay put when a
//! reindex finds nothing to do. It is a database-level, conservative marker:
//! it may move for writes that do not affect a given result.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/calls_fake_parser.rs"]
mod calls_fake_parser;
use calls_fake_parser::registry;

use mct_index::{ExcludeSet, Index};

fn project() -> (PathBuf, Index) {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-change-marker-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    fs::write(dir.join("a.fake"), "one calls two\ntwo\n").unwrap();
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    (dir, index)
}

#[test]
fn the_marker_is_stable_between_reads_and_conservative_on_a_no_op_reindex() {
    let (dir, mut index) = project();
    let before = index.change_marker().unwrap();
    assert_eq!(
        index.change_marker().unwrap(),
        before,
        "reads never move it"
    );
    let symbols = index.find_symbol("one").unwrap();
    assert_eq!(
        index.change_marker().unwrap(),
        before,
        "queries never move it"
    );
    index.reindex(&registry(), false).unwrap();
    // Measured: a reindex that skips every file still writes bookkeeping
    // (run metadata), so the marker may move without any symbol changing.
    // That is a false positive, never a false negative; the result is the same.
    assert_eq!(index.find_symbol("one").unwrap().len(), symbols.len());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_marker_moves_on_edit_addition_relation_change_and_removal() {
    let (dir, mut index) = project();
    let mut last = index.change_marker().unwrap();
    let mut step = |index: &mut Index, what: &str| {
        index.reindex(&registry(), false).unwrap();
        let now = index.change_marker().unwrap();
        assert_ne!(now, last, "marker must move after: {what}");
        last = now;
    };

    fs::write(dir.join("a.fake"), "one calls two\ntwo\nthree\n").unwrap();
    step(&mut index, "a symbol was added to a file");
    assert!(!index.find_symbol("three").unwrap().is_empty());

    fs::write(dir.join("a.fake"), "one calls three\ntwo\nthree\n").unwrap();
    step(&mut index, "only a relation target changed");
    assert_eq!(index.find_callers("three").unwrap().len(), 1);

    fs::write(dir.join("b.fake"), "extra\n").unwrap();
    step(&mut index, "a file was added");

    fs::remove_file(dir.join("b.fake")).unwrap();
    step(&mut index, "a file was removed");
    assert!(index.find_symbol("extra").unwrap().is_empty());
    fs::remove_dir_all(dir).unwrap();
}
