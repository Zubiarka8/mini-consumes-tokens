//! Atomic publication and crash recovery for a multi-file index run.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mct_core::{
    LanguageRegistry, Location, ParseError, ParsedFile, SourceFile, SymbolKind, SymbolRecord,
};
use mct_index::{ExcludeSet, Index};

struct Parser {
    entered: Option<Sender<()>>,
    release: Option<Mutex<Receiver<()>>>,
    crash_marker: Option<PathBuf>,
}

impl mct_core::LanguageParser for Parser {
    fn language_id(&self) -> &'static str {
        "fixture"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["gen"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        if file.relative_path == "b.gen" {
            if let Some(entered) = &self.entered {
                entered.send(()).expect("test receiver remains open");
                self.release
                    .as_ref()
                    .expect("release receiver exists")
                    .lock()
                    .expect("release lock")
                    .recv()
                    .expect("test releases parser");
            }
            if let Some(marker) = &self.crash_marker {
                fs::write(marker, "writer reached second file").expect("write test handshake");
                loop {
                    std::thread::park();
                }
            }
        }
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

fn registry(parser: Parser) -> LanguageRegistry {
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(parser));
    registry
}

fn tempdir() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    tempdir_with_nonce(nonce)
}

fn tempdir_with_nonce(nonce: u128) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mct-index-generation-{nonce}-{}-{serial}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return path.canonicalize().unwrap(),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("cannot create isolated generation fixture: {error}"),
        }
    }
}

#[test]
fn repeated_clock_ticks_do_not_share_fixture_databases() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let first = tempdir_with_nonce(nonce);
    let second = tempdir_with_nonce(nonce);
    assert_ne!(first, second, "parallel fixtures must not share a database");
}

fn database(root: &Path) -> PathBuf {
    root.join(".mct-index").join("index.sqlite3")
}

fn seed(root: &Path) {
    fs::write(root.join("a.gen"), "fn old_a\n").unwrap();
    fs::write(root.join("b.gen"), "fn old_b\n").unwrap();
    let mut index = Index::open(root, &database(root), ExcludeSet::default()).unwrap();
    index
        .reindex(
            &registry(Parser {
                entered: None,
                release: None,
                crash_marker: None,
            }),
            false,
        )
        .unwrap();
    assert_eq!(
        index.find_symbol("old_a").unwrap().len(),
        1,
        "seed {root:?}"
    );
    assert_eq!(
        index.find_symbol("old_b").unwrap().len(),
        1,
        "seed {root:?}"
    );
}

#[test]
fn generation_advances_only_when_a_complete_run_commits() {
    let root = tempdir();
    fs::write(root.join("a.gen"), "fn first\n").unwrap();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    assert_eq!(index.status().unwrap().published_generation, 0);

    index
        .reindex(
            &registry(Parser {
                entered: None,
                release: None,
                crash_marker: None,
            }),
            false,
        )
        .unwrap();
    let full = index.status().unwrap();
    assert_eq!(full.published_generation, 1);
    assert!(full.last_full_reconciled_at.is_some());

    fs::write(root.join("a.gen"), "fn second\n").unwrap();
    index
        .reindex_paths(
            &registry(Parser {
                entered: None,
                release: None,
                crash_marker: None,
            }),
            &[PathBuf::from("a.gen")],
        )
        .unwrap();
    let incremental = index.status().unwrap();
    assert_eq!(incremental.published_generation, 2);
    assert_eq!(
        incremental.last_full_reconciled_at,
        full.last_full_reconciled_at
    );
}

#[test]
fn independent_reader_sees_old_then_new_complete_generation() {
    let root = tempdir();
    seed(&root);
    fs::write(root.join("a.gen"), "fn new_a\n").unwrap();
    fs::write(root.join("b.gen"), "fn new_b\n").unwrap();
    let db = database(&root);
    let mut writer = Index::open(&root, &db, ExcludeSet::default()).unwrap();
    let reader = Index::open_existing_read_only(&root, &db, ExcludeSet::default()).unwrap();
    let generation = reader.status().unwrap().published_generation;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let parser = registry(Parser {
        entered: Some(entered_tx),
        release: Some(Mutex::new(release_rx)),
        crash_marker: None,
    });

    let update = std::thread::spawn(move || writer.reindex(&parser, true));
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(reader.status().unwrap().published_generation, generation);
    assert_eq!(
        reader.find_symbol("old_a").unwrap().len(),
        1,
        "indexed files: {}",
        reader.indexed_files().unwrap().len()
    );
    assert_eq!(reader.find_symbol("old_b").unwrap().len(), 1);
    assert!(reader.find_symbol("new_a").unwrap().is_empty());

    release_tx.send(()).unwrap();
    update.join().unwrap().unwrap();
    assert_eq!(
        reader.status().unwrap().published_generation,
        generation + 1
    );
    assert_eq!(reader.find_symbol("new_a").unwrap().len(), 1);
    assert_eq!(reader.find_symbol("new_b").unwrap().len(), 1);
    assert!(reader.find_symbol("old_a").unwrap().is_empty());
}

#[test]
fn navigation_snapshot_reuses_an_already_pinned_reader_transaction() {
    let root = tempdir();
    fs::write(root.join("a.gen"), "fn retained\n").unwrap();
    let registry = registry(Parser {
        entered: None,
        release: None,
        crash_marker: None,
    });
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();

    let _snapshot = index.read_snapshot().unwrap();
    let navigation = index.navigation_snapshot().unwrap();
    assert!(navigation
        .symbols
        .iter()
        .any(|symbol| symbol.name == "retained"));
}

#[test]
fn source_edit_during_parse_aborts_instead_of_publishing_stale_bytes() {
    let root = tempdir();
    seed(&root);
    fs::write(root.join("a.gen"), "fn new_a\n").unwrap();
    fs::write(root.join("b.gen"), "fn parsed_before_edit\n").unwrap();
    let db = database(&root);
    let mut writer = Index::open(&root, &db, ExcludeSet::default()).unwrap();
    let reader = Index::open_existing_read_only(&root, &db, ExcludeSet::default()).unwrap();
    let before = reader.status().unwrap().published_generation;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let parser = registry(Parser {
        entered: Some(entered_tx),
        release: Some(Mutex::new(release_rx)),
        crash_marker: None,
    });

    let update = std::thread::spawn(move || writer.reindex(&parser, true));
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    fs::write(root.join("b.gen"), "fn changed_while_parsing\n").unwrap();
    release_tx.send(()).unwrap();
    let error = update.join().unwrap().unwrap_err();
    assert!(error
        .to_string()
        .contains("source file changed while indexing"));

    assert_eq!(reader.status().unwrap().published_generation, before);
    assert_eq!(reader.find_symbol("old_a").unwrap().len(), 1);
    assert_eq!(reader.find_symbol("old_b").unwrap().len(), 1);
    assert!(reader.find_symbol("new_a").unwrap().is_empty());
    assert!(reader.find_symbol("parsed_before_edit").unwrap().is_empty());
}

#[test]
fn killed_writer_rolls_back_the_unpublished_generation_and_restart_reconciles() {
    let root = tempdir();
    seed(&root);
    fs::write(root.join("a.gen"), "fn new_a\n").unwrap();
    fs::write(root.join("b.gen"), "fn new_b\n").unwrap();
    let db = database(&root);
    let reader = Index::open_existing_read_only(&root, &db, ExcludeSet::default()).unwrap();
    let before = reader.status().unwrap().published_generation;
    let marker = root.join(".mct-index").join("writer-ready");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("killed_reindex_child")
        .arg("--nocapture")
        .env("MCT_INDEX_CRASH_ROOT", &root)
        .spawn()
        .unwrap();
    assert!(
        wait_for_file(&marker, &mut child),
        "child never reached the transaction barrier"
    );
    child.kill().unwrap();
    child.wait().unwrap();

    assert_eq!(reader.status().unwrap().published_generation, before);
    assert_eq!(reader.find_symbol("old_a").unwrap().len(), 1);
    assert_eq!(reader.find_symbol("old_b").unwrap().len(), 1);
    assert!(reader.find_symbol("new_a").unwrap().is_empty());

    let mut recovered = Index::open(&root, &db, ExcludeSet::default()).unwrap();
    recovered
        .reindex(
            &registry(Parser {
                entered: None,
                release: None,
                crash_marker: None,
            }),
            false,
        )
        .unwrap();
    assert_eq!(recovered.status().unwrap().published_generation, before + 1);
    assert_eq!(recovered.find_symbol("new_a").unwrap().len(), 1);
    assert_eq!(recovered.find_symbol("new_b").unwrap().len(), 1);
}

/// Subprocess target for the crash test. A marker is written only after the
/// first file's rows have been changed inside the still-open transaction.
#[test]
fn killed_reindex_child() {
    let Ok(root) = std::env::var("MCT_INDEX_CRASH_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mut index = Index::open(&root, &database(&root), ExcludeSet::default()).unwrap();
    index
        .reindex(
            &registry(Parser {
                entered: None,
                release: None,
                crash_marker: Some(root.join(".mct-index").join("writer-ready")),
            }),
            true,
        )
        .unwrap();
}

fn wait_for_file(path: &Path, child: &mut Child) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        if child.try_wait().unwrap().is_some() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}
