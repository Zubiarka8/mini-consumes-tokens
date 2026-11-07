//! A reindex that finds nothing to change publishes nothing: no generation
//! bump, no `index_meta` write, no WAL frame, no post-reindex hook. Other
//! processes key their caches (and the dashboard its snapshot) on what it
//! would otherwise write.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use mct_core::{
    LanguageRegistry, Location, ParseError, ParsedFile, SourceFile, SymbolKind, SymbolRecord,
};
use mct_index::{ExcludeSet, Index};

struct Parser;

impl mct_core::LanguageParser for Parser {
    fn language_id(&self) -> &'static str {
        "fixture"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["gen"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let name = file
            .contents
            .trim()
            .strip_prefix("fn ")
            .ok_or_else(|| ParseError::Syntax {
                path: file.relative_path.clone(),
                line: 1,
                message: "expected fn declaration".into(),
            })?
            .to_string();
        Ok(ParsedFile {
            symbols: vec![SymbolRecord {
                id: 0,
                name,
                kind: SymbolKind::Function,
                location: Location {
                    line: 1,
                    column: 1,
                    byte_len: file.contents.len() as u32,
                    end_line: Some(1),
                },
                parent: None,
                level: None,
            }],
            ..Default::default()
        })
    }
}

fn registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(Parser));
    registry
}

fn database(root: &Path) -> PathBuf {
    root.join(".mct-index").join("index.sqlite3")
}

/// Every kind of row a steady-state walk re-reads: a parsed source file, a
/// syntax error, a file of a known-but-unsupported language, and manifests of
/// two ecosystems plus a rockspec (re-parsed on every walk, never
/// hash-skipped).
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("a.gen"), "fn alpha\n").unwrap();
    fs::write(root.join("bad.gen"), "not a declaration\n").unwrap();
    fs::write(root.join("view.swift"), "struct V {}\n").unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"p\"\n\n[dependencies]\nserde = \"1\"\nanyhow = \"1.0\"\n",
    )
    .unwrap();
    fs::write(
        root.join("package.json"),
        r#"{"dependencies": {"left-pad": "^1.3.0"}}"#,
    )
    .unwrap();
    fs::write(
        root.join("demo-1.0-1.rockspec"),
        "package = \"demo\"\nversion = \"1.0-1\"\ndependencies = { \"lua >= 5.1\" }\n",
    )
    .unwrap();
    dir
}

fn wal_stamp(root: &Path) -> Option<(SystemTime, u64)> {
    let mut wal = database(root).into_os_string();
    wal.push("-wal");
    let meta = fs::metadata(wal).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

#[test]
fn an_unchanged_full_reindex_publishes_nothing() {
    let dir = project();
    let root = dir.path().canonicalize().unwrap();
    let registry = registry();
    let mut index = Index::open(&root, &database(&root), ExcludeSet::default()).unwrap();

    let first = index.reindex(&registry, false).unwrap();
    assert!(first.published);
    let generation = index.published_generation().unwrap();
    let indexed_at = index.status().unwrap().last_indexed_at;
    let wal = wal_stamp(&root);

    for _ in 0..2 {
        let report = index.reindex(&registry, false).unwrap();
        assert!(!report.published, "{report:?}");
        assert_eq!(index.published_generation().unwrap(), generation);
        assert_eq!(wal_stamp(&root), wal, "an unchanged run wrote the WAL");
    }
    let status = index.status().unwrap();
    assert_eq!(status.last_indexed_at, indexed_at);
    assert!(status.last_verified_at.is_some());
    assert!(status.last_full_reconciled_at.is_some());
}

#[test]
fn an_unchanged_incremental_reindex_publishes_nothing() {
    let dir = project();
    let root = dir.path().canonicalize().unwrap();
    let registry = registry();
    let mut index = Index::open(&root, &database(&root), ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    let generation = index.published_generation().unwrap();
    let wal = wal_stamp(&root);

    let paths = [
        "a.gen",
        "bad.gen",
        "view.swift",
        "Cargo.toml",
        "package.json",
        "demo-1.0-1.rockspec",
        "deleted-long-ago.gen",
    ]
    .map(PathBuf::from);
    let report = index.reindex_paths(&registry, &paths).unwrap();
    assert!(!report.published, "{report:?}");
    assert_eq!(index.published_generation().unwrap(), generation);
    assert_eq!(wal_stamp(&root), wal);
}

#[test]
fn each_kind_of_change_still_publishes_once() {
    let dir = project();
    let root = dir.path().canonicalize().unwrap();
    let registry = registry();
    let mut index = Index::open(&root, &database(&root), ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();

    let edits: [&dyn Fn(&Path); 7] = [
        &|root| fs::write(root.join("a.gen"), "fn alpha_two\n").unwrap(),
        &|root| fs::write(root.join("bad.gen"), "fn fixed\n").unwrap(),
        &|root| {
            fs::write(
                root.join("Cargo.toml"),
                "[package]\nname = \"p\"\n\n[dependencies]\nserde = \"2\"\n",
            )
            .unwrap()
        },
        &|root| {
            fs::write(
                root.join("demo-1.0-1.rockspec"),
                "package = \"demo\"\nversion = \"1.0-1\"\ndependencies = { \"lua >= 5.4\" }\n",
            )
            .unwrap()
        },
        &|root| fs::remove_file(root.join("view.swift")).unwrap(),
        &|root| fs::remove_file(root.join("package.json")).unwrap(),
        &|root| fs::remove_file(root.join("a.gen")).unwrap(),
    ];
    for (n, edit) in edits.iter().enumerate() {
        let before = index.published_generation().unwrap();
        edit(&root);
        let report = index.reindex(&registry, false).unwrap();
        assert!(report.published, "edit {n}: {report:?}");
        assert_eq!(
            index.published_generation().unwrap(),
            before + 1,
            "edit {n}"
        );
        let again = index.reindex(&registry, false).unwrap();
        assert!(!again.published, "edit {n} settled: {again:?}");
    }
    assert!(index.find_symbol("alpha_two").unwrap().is_empty());
    assert_eq!(index.find_symbol("fixed").unwrap().len(), 1);
}

#[test]
fn a_new_empty_index_is_published_once() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut index = Index::open(&root, &database(&root), ExcludeSet::default()).unwrap();
    assert!(index.reindex(&registry(), false).unwrap().published);
    assert_eq!(index.published_generation().unwrap(), 1);
    assert!(index.status().unwrap().last_indexed_at.is_some());
    assert!(!index.reindex(&registry(), false).unwrap().published);
}

#[test]
fn the_hook_runs_once_per_process_then_only_after_a_change() {
    let dir = project();
    let root = dir.path().canonicalize().unwrap();
    let registry = registry();
    {
        let mut setup = Index::open(&root, &database(&root), ExcludeSet::default()).unwrap();
        setup.reindex(&registry, false).unwrap();
    }

    // A new process on an up-to-date index: its first run changes nothing
    // but still writes the derived views once.
    let mut index = Index::open(&root, &database(&root), ExcludeSet::default()).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    index.set_post_reindex_hook(Box::new(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));
    assert!(!index.reindex(&registry, false).unwrap().published);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    index.reindex(&registry, false).unwrap();
    index
        .reindex_paths(&registry, &[PathBuf::from("a.gen")])
        .unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "no-ops must not run the hook"
    );

    fs::write(root.join("a.gen"), "fn alpha_three\n").unwrap();
    index
        .reindex_paths(&registry, &[PathBuf::from("a.gen")])
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
