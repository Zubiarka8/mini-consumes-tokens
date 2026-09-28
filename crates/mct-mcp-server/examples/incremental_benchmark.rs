//! Incremental indexing benchmark (issue #25), separate from the functional
//! tests in `crates/mct-index/tests/incremental.rs`. Copies a real source
//! tree (this repository's `crates/` by default) into a temp directory, builds
//! an on-disk index of it (WAL, as the server runs), then times, per change:
//!
//! - a full non-forced `reindex` with one file changed — what the watcher
//!   used to run on every settled change;
//! - `reindex_paths` for that one file — what it runs now;
//! - `reindex_paths` for a created and for a deleted file.
//!
//! Exits non-zero when the single-file-edit p95 is ≥100 ms.
//!
//! ```sh
//! cargo run --release -p mct-mcp-server --example incremental_benchmark [-- <source dir>]
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use mct_index::{ExcludeSet, Index};

const SAMPLES: usize = 30;

fn copy_tree(from: &Path, to: &Path) -> usize {
    let mut files = 0;
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().filter_map(Result::ok) {
        let name = entry.file_name();
        if name == "target" || name.to_string_lossy().starts_with('.') {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        match entry.file_type() {
            Ok(t) if t.is_dir() => files += copy_tree(&src, &dst),
            Ok(t) if t.is_file() => {
                fs::copy(&src, &dst).unwrap();
                files += 1;
            }
            _ => {}
        }
    }
    files
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn summary(label: &str, samples: &mut [Duration]) -> Duration {
    samples.sort();
    let at = |p: f64| samples[((samples.len() - 1) as f64 * p).round() as usize];
    println!(
        "{label}: p50 {:.2} ms, p95 {:.2} ms, max {:.2} ms (n={})",
        ms(at(0.5)),
        ms(at(0.95)),
        ms(at(1.0)),
        samples.len()
    );
    at(0.95)
}

fn main() {
    let source = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."))
        .canonicalize()
        .unwrap();
    let work = std::env::temp_dir().join(format!("mct-incremental-bench-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    let root = work.join("project");
    let copied = copy_tree(&source, &root);
    let root = root.canonicalize().unwrap();

    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open(&root, &work.join("index.sqlite3"), ExcludeSet::default()).unwrap();
    let start = Instant::now();
    let initial = index.reindex(&registry, false).unwrap();
    println!(
        "# Incremental indexing benchmark — copy of {}",
        source.display()
    );
    println!(
        "{copied} files copied; initial full index: {} files, {} symbols in {:.0} ms (on-disk SQLite, WAL)",
        initial.files_parsed,
        initial.symbols_written,
        ms(start.elapsed())
    );

    // Rust files the index actually holds (not excluded, parsed cleanly).
    let mut candidates: Vec<PathBuf> = index
        .list_symbols_all(None, Some("rust"))
        .unwrap()
        .into_iter()
        .map(|s| root.join(s.relative_path))
        .collect();
    candidates.sort();
    candidates.dedup();
    let step = (candidates.len() / SAMPLES).max(1);
    let targets: Vec<&PathBuf> = candidates.iter().step_by(step).take(SAMPLES).collect();

    let (mut full, mut incremental, mut created, mut deleted) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (i, path) in targets.iter().enumerate() {
        // Full walk after a one-file edit: the previous watcher behaviour.
        let original = fs::read_to_string(path).unwrap();
        fs::write(path, format!("{original}\npub fn bench_full_{i}() {{}}\n")).unwrap();
        let t = Instant::now();
        let report = index.reindex(&registry, false).unwrap();
        full.push(t.elapsed());
        assert_eq!(report.files_parsed, 1);

        // The same kind of edit, updated incrementally.
        fs::write(path, format!("{original}\npub fn bench_incr_{i}() {{}}\n")).unwrap();
        let t = Instant::now();
        let report = index
            .reindex_paths(&registry, std::slice::from_ref(*path))
            .unwrap();
        incremental.push(t.elapsed());
        assert_eq!(report.files_parsed, 1);
        assert_eq!(
            index.find_symbol(&format!("bench_incr_{i}")).unwrap().len(),
            1
        );
        assert!(index
            .find_symbol(&format!("bench_full_{i}"))
            .unwrap()
            .is_empty());

        // A new file next to it, then its deletion.
        let new_file = path.with_file_name(format!("bench_new_{i}.rs"));
        fs::write(&new_file, format!("pub fn bench_new_{i}() {{}}\n")).unwrap();
        let t = Instant::now();
        index
            .reindex_paths(&registry, std::slice::from_ref(&new_file))
            .unwrap();
        created.push(t.elapsed());
        fs::remove_file(&new_file).unwrap();
        let t = Instant::now();
        index.reindex_paths(&registry, &[new_file]).unwrap();
        deleted.push(t.elapsed());
        assert!(index
            .find_symbol(&format!("bench_new_{i}"))
            .unwrap()
            .is_empty());

        fs::write(path, original).unwrap();
        index
            .reindex_paths(&registry, std::slice::from_ref(*path))
            .unwrap();
    }

    println!();
    let full_p95 = summary("full reindex, 1 file edited (before)", &mut full);
    let edit_p95 = summary("reindex_paths, 1 file edited", &mut incremental);
    summary("reindex_paths, 1 file created", &mut created);
    summary("reindex_paths, 1 file deleted", &mut deleted);
    println!(
        "\nspeed-up at p95: {:.1}x",
        full_p95.as_secs_f64() / edit_p95.as_secs_f64().max(f64::EPSILON)
    );

    let _ = fs::remove_dir_all(&work);
    if edit_p95 >= Duration::from_millis(100) {
        println!(
            "FAIL: single-file update p95 {:.2} ms ≥ 100 ms",
            ms(edit_p95)
        );
        std::process::exit(1);
    }
    println!("PASS: single-file update p95 < 100 ms");
}
