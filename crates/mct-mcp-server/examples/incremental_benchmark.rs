//! Incremental indexing benchmark (issue #25), separate from the functional
//! tests in `crates/mct-index/tests/incremental.rs`. Copies a real source
//! tree (this repository's `crates/` by default) into a temp directory, builds
//! an on-disk index of it (WAL, as the server runs), then times, per change:
//!
//! - a full non-forced `reindex` with one file changed — what the watcher
//!   used to run on every settled change;
//! - `reindex_paths` for that one file — what it runs now;
//! - `reindex_paths` for a created file, for its rename reported by the new
//!   path only (as macOS FSEvents delivers it), and for its deletion — the
//!   three that also sweep the index for vanished files.
//!
//! `--synthetic N` generates N small Rust files (100 per directory) instead,
//! for the cost of that sweep on a large repository.
//!
//! Exits non-zero when the single-file-edit p95 is ≥100 ms.
//!
//! ```sh
//! cargo run --release -p mct-mcp-server --example incremental_benchmark [-- <source dir>]
//! cargo run --release -p mct-mcp-server --example incremental_benchmark -- --synthetic 50000
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

/// `count` Rust files of a few related functions each, 100 per directory.
fn generate_tree(to: &Path, count: usize) -> usize {
    for i in 0..count {
        let dir = to.join(format!("src/m{:04}", i / 100));
        if i % 100 == 0 {
            fs::create_dir_all(&dir).unwrap();
        }
        let prev = i.saturating_sub(1);
        fs::write(
            dir.join(format!("f{i}.rs")),
            format!(
                "use crate::m{:04}::f{prev};\n\n\
                 /// Item {i}.\n\
                 pub struct Item{i} {{ pub value: u64 }}\n\n\
                 impl Item{i} {{\n    pub fn new(value: u64) -> Self {{ Self {{ value }} }}\n\
                 \x20   pub fn double(&self) -> u64 {{ helper_{i}(self.value) * 2 }}\n}}\n\n\
                 fn helper_{i}(v: u64) -> u64 {{ f{prev}::helper_{prev}(v) + 1 }}\n",
                prev / 100
            ),
        )
        .unwrap();
    }
    count
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let work = std::env::temp_dir().join(format!("mct-incremental-bench-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    let root = work.join("project");
    let (label, copied) = if args.first().map(String::as_str) == Some("--synthetic") {
        let count = args.get(1).and_then(|n| n.parse().ok()).unwrap_or(50_000);
        (
            format!("synthetic tree of {count} Rust files"),
            generate_tree(&root, count),
        )
    } else {
        let source = args
            .first()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."))
            .canonicalize()
            .unwrap();
        (
            format!("copy of {}", source.display()),
            copy_tree(&source, &root),
        )
    };
    let root = root.canonicalize().unwrap();

    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open(&root, &work.join("index.sqlite3"), ExcludeSet::default()).unwrap();
    let start = Instant::now();
    let initial = index.reindex(&registry, false).unwrap();
    println!("# Incremental indexing benchmark — {label}");
    println!(
        "{copied} files; initial full index: {} files, {} symbols in {:.0} ms (on-disk SQLite, WAL)",
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

    let (mut full, mut incremental, mut created, mut renamed, mut deleted) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
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

        // A new file next to it, its rename (new path only), its deletion.
        let first_name = path.with_file_name(format!("bench_first_{i}.rs"));
        fs::write(&first_name, format!("pub fn bench_new_{i}() {{}}\n")).unwrap();
        let t = Instant::now();
        index
            .reindex_paths(&registry, std::slice::from_ref(&first_name))
            .unwrap();
        created.push(t.elapsed());
        let new_file = path.with_file_name(format!("bench_new_{i}.rs"));
        fs::rename(&first_name, &new_file).unwrap();
        let t = Instant::now();
        let report = index
            .reindex_paths(&registry, std::slice::from_ref(&new_file))
            .unwrap();
        renamed.push(t.elapsed());
        assert_eq!(report.files_removed, 1);
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
    summary(
        "reindex_paths, 1 file renamed (new path only)",
        &mut renamed,
    );
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
