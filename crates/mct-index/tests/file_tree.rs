//! Covers `Index::file_tree`: a plain filesystem walk for navigation,
//! independent of the symbol index — depth limiting, `ExcludeSet` pruning,
//! the per-directory entry cap, and the root-escape/not-a-directory guards.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use mct_index::{ExcludeSet, Index, IndexError};

#[test]
fn lists_files_and_directories_sorted_dirs_first() {
    let dir = tempdir();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/lib.rs"), "").unwrap();
    fs::write(dir.join("Cargo.toml"), "").unwrap();
    fs::write(dir.join("README.md"), "").unwrap();

    let index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let tree = index.file_tree(None, 3).unwrap();

    assert!(tree.is_dir);
    let names: Vec<&str> = tree.children.iter().map(|c| c.name.as_str()).collect();
    // Directories sort before files; alphabetical within each group.
    assert_eq!(names, ["src", "Cargo.toml", "README.md"]);
    assert!(tree.children[0].is_dir);
    assert!(!tree.children[1].is_dir);

    let src = &tree.children[0];
    assert_eq!(src.children.len(), 1);
    assert_eq!(src.children[0].name, "lib.rs");
}

#[test]
fn excluded_directories_never_appear() {
    let dir = tempdir();
    fs::create_dir_all(dir.join("target/debug")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("target/debug/build.bin"), "").unwrap();
    fs::write(dir.join("src/main.rs"), "").unwrap();

    let index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let tree = index.file_tree(None, 5).unwrap();

    let names: Vec<&str> = tree.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["src"], "target/ is excluded by default, like reindexing");
}

#[test]
fn depth_stops_recursion_and_flags_it() {
    let dir = tempdir();
    fs::create_dir_all(dir.join("a/b/c")).unwrap();
    fs::write(dir.join("a/b/c/deep.txt"), "").unwrap();

    let index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let tree = index.file_tree(None, 2).unwrap();

    let a = tree.children.iter().find(|c| c.name == "a").unwrap();
    assert!(!a.depth_exhausted, "depth 2 still descends into `a`");
    let b = a.children.iter().find(|c| c.name == "b").unwrap();
    // `b` is the second level (depth budget exhausted before listing `c`'s
    // own children): its own name shows up, but its non-empty contents don't.
    assert!(b.children.is_empty());
    assert!(b.depth_exhausted, "`b/c` exists but wasn't listed");
}

#[test]
fn scoping_to_a_subdirectory_roots_the_tree_there() {
    let dir = tempdir();
    fs::create_dir_all(dir.join("crates/foo/src")).unwrap();
    fs::write(dir.join("crates/foo/src/lib.rs"), "").unwrap();
    fs::create_dir_all(dir.join("crates/bar")).unwrap();

    let index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let tree = index.file_tree(Some("crates/foo"), 3).unwrap();

    assert_eq!(tree.name, "foo");
    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].name, "src");
}

#[test]
fn a_path_outside_the_root_is_rejected() {
    let dir = tempdir();
    fs::create_dir_all(dir.join("inner")).unwrap();
    let index = Index::open_in_memory(&dir.join("inner"), ExcludeSet::default()).unwrap();

    let err = index.file_tree(Some(".."), 3).unwrap_err();
    assert!(matches!(err, IndexError::PathEscapesRoot(_)), "{err:?}");
}

#[test]
fn a_file_path_is_rejected_not_a_directory() {
    let dir = tempdir();
    fs::write(dir.join("plain.txt"), "").unwrap();
    let index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();

    let err = index.file_tree(Some("plain.txt"), 3).unwrap_err();
    assert!(matches!(err, IndexError::NotADirectory(_)), "{err:?}");
}

#[test]
fn a_directory_over_the_entry_cap_reports_how_many_were_omitted() {
    let dir = tempdir();
    fs::create_dir_all(dir.join("many")).unwrap();
    for i in 0..201 {
        fs::write(dir.join(format!("many/f{i:03}.txt")), "").unwrap();
    }

    let index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    let tree = index.file_tree(None, 2).unwrap();
    let many = tree.children.iter().find(|c| c.name == "many").unwrap();

    assert_eq!(many.children.len(), 200, "capped at the per-directory limit");
    assert_eq!(many.omitted, 1);
}

// --- harness -------------------------------------------------------------

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-index-file-tree-test-{}", uuid_like()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn uuid_like() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}
