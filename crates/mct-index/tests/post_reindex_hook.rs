#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, IndexError};

fn project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-index-hook-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_hook_runs_after_reindex_and_reindex_paths() {
    let dir = project("runs");
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    index.set_post_reindex_hook(Box::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));
    let registry = LanguageRegistry::new();

    index.reindex(&registry, false).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    index.reindex_paths(&registry, &[dir.join("a.rs")]).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn reindex_paths_falling_back_to_a_full_reindex_runs_the_hook_once() {
    let dir = project("once");
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    index.set_post_reindex_hook(Box::new(move |_| {
        seen.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));

    // The root itself is "everything": handled by a full reindex internally.
    index
        .reindex_paths(&LanguageRegistry::new(), std::slice::from_ref(&dir))
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_failing_hook_becomes_a_warning_and_the_reindex_still_succeeds() {
    let dir = project("fails");
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.set_post_reindex_hook(Box::new(|_| Err(IndexError::Hook("disk full".into()))));

    let report = index.reindex(&LanguageRegistry::new(), false).unwrap();
    assert_eq!(report.warnings.len(), 1);
    assert!(
        report.warnings[0].contains("disk full"),
        "{:?}",
        report.warnings
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn without_a_hook_there_are_no_warnings() {
    let dir = project("none");
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let report = index.reindex(&LanguageRegistry::new(), false).unwrap();
    assert!(report.warnings.is_empty());
    fs::remove_dir_all(&dir).ok();
}
