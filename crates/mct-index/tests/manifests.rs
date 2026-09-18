//! End-to-end test of manifest-based dependency detection through the real
//! `reindex`/`status` pipeline — `crates/mct-index/src/manifests.rs` already
//! has unit tests for each format's parsing; this file only exercises how
//! detected dependencies flow into the database and `IndexStatus`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};

/// No `LanguageParser` needed — manifest detection runs independently of the
/// registry, before any extension/parser lookup.
fn empty_registry() -> LanguageRegistry {
    LanguageRegistry::new()
}

#[test]
fn cargo_toml_dependencies_appear_in_status() {
    let dir = tempdir();
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"demo\"\n\n[dependencies]\nthiserror = \"2\"\n",
    )
    .unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&empty_registry(), false).unwrap();

    let status = index.status().unwrap();
    assert_eq!(status.dependencies.len(), 1);
    let manifest = &status.dependencies[0];
    assert_eq!(manifest.manifest_path, "Cargo.toml");
    assert_eq!(manifest.language, "rust");
    assert!(manifest
        .dependencies
        .iter()
        .any(|d| d.name == "thiserror" && d.version.as_deref() == Some("2")));
}

#[test]
fn multiple_manifests_are_grouped_separately() {
    let dir = tempdir();
    fs::write(
        dir.join("Cargo.toml"),
        "[dependencies]\nthiserror = \"2\"\n",
    )
    .unwrap();
    fs::write(
        dir.join("package.json"),
        r#"{"dependencies": {"react": "18.3.1"}}"#,
    )
    .unwrap();
    fs::write(dir.join("requirements.txt"), "flask==2.3.0\n").unwrap();
    fs::write(
        dir.join("go.mod"),
        "module example.com/app\n\nrequire github.com/foo/bar v1.0.0\n",
    )
    .unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&empty_registry(), false).unwrap();

    let status = index.status().unwrap();
    assert_eq!(status.dependencies.len(), 4, "one group per manifest file: {:?}", status.dependencies);

    let languages: Vec<&str> = status.dependencies.iter().map(|m| m.language.as_str()).collect();
    assert!(languages.contains(&"rust"));
    assert!(languages.contains(&"javascript_typescript"));
    assert!(languages.contains(&"python"));
    assert!(languages.contains(&"go"));
}

#[test]
fn removing_a_manifest_removes_its_dependencies_on_reindex() {
    let dir = tempdir();
    let manifest_path = dir.join("package.json");
    fs::write(&manifest_path, r#"{"dependencies": {"react": "18.3.1"}}"#).unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&empty_registry(), false).unwrap();
    assert_eq!(index.status().unwrap().dependencies.len(), 1);

    fs::remove_file(&manifest_path).unwrap();
    index.reindex(&empty_registry(), false).unwrap();
    assert!(index.status().unwrap().dependencies.is_empty());
}

#[test]
fn editing_a_manifest_updates_dependencies_on_reindex() {
    let dir = tempdir();
    let manifest_path = dir.join("requirements.txt");
    fs::write(&manifest_path, "flask==2.3.0\n").unwrap();

    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(&empty_registry(), false).unwrap();
    let first = index.status().unwrap();
    assert_eq!(first.dependencies[0].dependencies.len(), 1);

    fs::write(&manifest_path, "flask==2.3.0\nrequests==2.31.0\n").unwrap();
    index.reindex(&empty_registry(), false).unwrap();
    let second = index.status().unwrap();
    assert_eq!(second.dependencies[0].dependencies.len(), 2, "manifests are re-parsed every reindex, not hash-skipped");
}

/// A fresh temp directory, canonicalized so it matches what `Index::open_in_memory`
/// stores as `root` after its own canonicalization.
fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-index-manifests-test-{}", uuid_like()));
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
