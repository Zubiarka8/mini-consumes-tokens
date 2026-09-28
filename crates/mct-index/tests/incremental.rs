//! `Index::reindex_paths` (issue #25): updating only the paths a watcher
//! reported must leave exactly the index a full reindex would build — no
//! stale symbols, relations, issues or dependencies — while touching nothing
//! else. Every scenario ends by comparing against a fresh full index of the
//! same directory. Latency is measured separately, by
//! `crates/mct-mcp-server/examples/incremental_benchmark.rs`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::{
    Location, ParseError, ParsedFile, RelationKind, SourceFile, SymbolKind, SymbolRecord,
    SymbolRelation,
};
use mct_index::{ExcludeSet, Index};

/// Toy format: each line is `fn NAME` or `fn NAME calls OTHER`; anything else
/// is a syntax error.
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
            let location = Location {
                line: line_no as u32 + 1,
                column: 1,
                byte_len: line.len() as u32,
                end_line: Some(line_no as u32 + 1),
            };
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name: parts.next().unwrap_or_default().to_string(),
                kind: SymbolKind::Function,
                location,
                parent: None,
                level: None,
            });
            if parts.next() == Some("calls") {
                if let Some(callee) = parts.next() {
                    parsed.relations.push(SymbolRelation {
                        from: id,
                        kind: RelationKind::Calls,
                        to_name: callee.to_string(),
                        location,
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
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-incremental-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn write(root: &Path, rel: &str, contents: &str) -> PathBuf {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    path
}

fn indexed(root: &Path) -> Index {
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    index
}

/// Everything a query can observe: every symbol, every relation count by
/// name, the file count, syntax errors and manifest dependencies.
fn snapshot(index: &Index) -> String {
    let mut out = String::new();
    let mut symbols: Vec<String> = index
        .list_symbols_all(None, None)
        .unwrap()
        .into_iter()
        .map(|s| {
            format!(
                "{}:{}-{:?} {} {} {}",
                s.relative_path, s.line, s.end_line, s.language, s.kind, s.name
            )
        })
        .collect();
    symbols.sort();
    out.push_str(&symbols.join("\n"));
    let mut refs: Vec<_> = index.reference_counts().unwrap().into_iter().collect();
    refs.sort();
    out.push_str(&format!("\nrefs {refs:?}"));
    let mut callers: Vec<_> = index.fan_in_counts().unwrap().into_iter().collect();
    callers.sort();
    out.push_str(&format!("\ncallers {callers:?}"));
    let status = index.status().unwrap();
    let mut errors: Vec<_> = status
        .syntax_errors
        .iter()
        .map(|e| format!("{} {}", e.relative_path, e.detail))
        .collect();
    errors.sort();
    let mut deps: Vec<_> = status
        .dependencies
        .iter()
        .map(|m| {
            let mut names: Vec<_> = m.dependencies.iter().map(|d| d.name.clone()).collect();
            names.sort();
            format!("{} {names:?}", m.manifest_path)
        })
        .collect();
    deps.sort();
    out.push_str(&format!(
        "\nfiles {} symbols {} errors {errors:?} deps {deps:?}",
        status.total_files, status.total_symbols
    ));
    out
}

/// The incremental index must equal a fresh full index of the same tree.
fn assert_matches_full(index: &Index, root: &Path) {
    assert_eq!(snapshot(index), snapshot(&indexed(root)));
}

fn project() -> PathBuf {
    let root = tempdir();
    write(
        &root,
        "src/main.fake",
        "fn main calls helper\nfn run calls helper\n",
    );
    write(&root, "src/util.fake", "fn helper\nfn unused\n");
    write(&root, "lib/extra.fake", "fn extra calls main\n");
    root
}

#[test]
fn an_edited_file_is_reparsed_alone_and_its_stale_symbols_dropped() {
    let root = project();
    let mut index = indexed(&root);
    let path = write(
        &root,
        "src/util.fake",
        "fn helper calls extra\nfn renamed\n",
    );

    let report = index.reindex_paths(&registry(), &[path]).unwrap();
    assert_eq!((report.files_parsed, report.files_unchanged), (1, 0));
    assert!(index.find_symbol("unused").unwrap().is_empty());
    assert_eq!(index.find_symbol("renamed").unwrap().len(), 1);
    assert_eq!(index.find_callers("extra").unwrap().len(), 1);
    // Callers in *other* files still resolve to the rewritten definition.
    assert_eq!(index.find_callers("helper").unwrap().len(), 2);
    assert_matches_full(&index, &root);
}

#[test]
fn an_unchanged_file_is_hash_skipped() {
    let root = project();
    let mut index = indexed(&root);
    let report = index
        .reindex_paths(&registry(), &[root.join("src/main.fake")])
        .unwrap();
    assert_eq!((report.files_parsed, report.files_unchanged), (0, 1));
}

#[test]
fn created_and_deleted_files_are_added_and_dropped() {
    let root = project();
    let mut index = indexed(&root);
    let created = write(&root, "src/new.fake", "fn fresh calls helper\n");
    let deleted = root.join("lib/extra.fake");
    fs::remove_file(&deleted).unwrap();

    let report = index
        .reindex_paths(&registry(), &[created, deleted])
        .unwrap();
    assert_eq!((report.files_parsed, report.files_removed), (1, 1));
    assert!(index.find_symbol("extra").unwrap().is_empty());
    assert_eq!(index.find_symbol("fresh").unwrap().len(), 1);
    assert_matches_full(&index, &root);
}

#[test]
fn a_renamed_file_moves_its_symbols() {
    let root = project();
    let mut index = indexed(&root);
    let from = root.join("src/util.fake");
    let to = root.join("src/helpers.fake");
    fs::rename(&from, &to).unwrap();

    // A watcher reports a rename as both paths.
    index.reindex_paths(&registry(), &[from, to]).unwrap();
    let hits = index.find_symbol("helper").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].relative_path, "src/helpers.fake");
    assert_matches_full(&index, &root);
}

#[test]
fn directories_created_deleted_or_renamed_are_handled_as_subtrees() {
    let root = project();
    let mut index = indexed(&root);

    // Created with content, reported as the directory only.
    write(&root, "pkg/a.fake", "fn pkg_a\n");
    write(&root, "pkg/deep/b.fake", "fn pkg_b calls pkg_a\n");
    index
        .reindex_paths(&registry(), &[root.join("pkg")])
        .unwrap();
    assert_eq!(index.find_symbol("pkg_b").unwrap().len(), 1);
    assert_matches_full(&index, &root);

    // Renamed: old directory gone, new one in place.
    fs::rename(root.join("pkg"), root.join("moved")).unwrap();
    index
        .reindex_paths(&registry(), &[root.join("pkg"), root.join("moved")])
        .unwrap();
    assert_eq!(
        index.find_symbol("pkg_a").unwrap()[0].relative_path,
        "moved/a.fake"
    );
    assert_matches_full(&index, &root);

    // Deleted outright.
    fs::remove_dir_all(root.join("moved")).unwrap();
    let report = index
        .reindex_paths(&registry(), &[root.join("moved")])
        .unwrap();
    assert_eq!(report.files_removed, 2);
    assert_matches_full(&index, &root);
}

#[test]
fn a_directory_prefix_matches_whole_path_components_only() {
    let root = project();
    // `src_old` and `src%` share a string prefix with `src`, and `_`/`%` are
    // `LIKE` wildcards: none of them may be swept with `src`.
    write(&root, "src_old/keep.fake", "fn keep_underscore\n");
    write(&root, "src%/keep.fake", "fn keep_percent\n");
    let mut index = indexed(&root);
    fs::remove_dir_all(root.join("src")).unwrap();
    index
        .reindex_paths(&registry(), &[root.join("src")])
        .unwrap();
    assert!(index.find_symbol("main").unwrap().is_empty());
    assert_eq!(index.find_symbol("keep_underscore").unwrap().len(), 1);
    assert_eq!(index.find_symbol("keep_percent").unwrap().len(), 1);
    assert_matches_full(&index, &root);
}

#[test]
fn syntax_errors_are_recorded_and_cleared_like_a_full_reindex() {
    let root = project();
    let mut index = indexed(&root);
    let path = write(&root, "src/broken.fake", "this is not fn\n");
    index
        .reindex_paths(&registry(), std::slice::from_ref(&path))
        .unwrap();
    assert_eq!(index.status().unwrap().syntax_errors.len(), 1);
    assert_matches_full(&index, &root);

    write(&root, "src/broken.fake", "fn fixed\n");
    index
        .reindex_paths(&registry(), std::slice::from_ref(&path))
        .unwrap();
    assert!(index.status().unwrap().syntax_errors.is_empty());
    assert_matches_full(&index, &root);

    fs::write(&path, "still not fn\n").unwrap();
    index
        .reindex_paths(&registry(), std::slice::from_ref(&path))
        .unwrap();
    fs::remove_file(&path).unwrap();
    index.reindex_paths(&registry(), &[path]).unwrap();
    assert!(index.status().unwrap().syntax_errors.is_empty());
    assert_matches_full(&index, &root);
}

#[test]
fn manifests_are_updated_and_dropped() {
    let root = project();
    let mut index = indexed(&root);
    let manifest = write(
        &root,
        "Cargo.toml",
        "[package]\nname = \"x\"\n\n[dependencies]\nserde = \"1\"\n",
    );
    index
        .reindex_paths(&registry(), std::slice::from_ref(&manifest))
        .unwrap();
    assert_eq!(index.status().unwrap().dependencies.len(), 1);
    assert_matches_full(&index, &root);

    fs::remove_file(&manifest).unwrap();
    index.reindex_paths(&registry(), &[manifest]).unwrap();
    assert!(index.status().unwrap().dependencies.is_empty());
    assert_matches_full(&index, &root);
}

#[test]
fn excluded_and_outside_paths_are_ignored() {
    let root = project();
    let mut index = indexed(&root);
    let excluded = write(&root, "target/debug/gen.fake", "fn generated\n");
    let outside = tempdir().join("elsewhere.fake");
    fs::write(&outside, "fn outsider\n").unwrap();
    let report = index
        .reindex_paths(&registry(), &[excluded, outside])
        .unwrap();
    assert_eq!(report.files_parsed, 0);
    assert!(index.find_symbol("generated").unwrap().is_empty());
    assert!(index.find_symbol("outsider").unwrap().is_empty());
    assert_matches_full(&index, &root);
}

#[cfg(unix)]
#[test]
fn a_symlink_escaping_the_root_is_never_indexed_and_drops_its_old_rows() {
    let root = project();
    let outside = tempdir();
    fs::write(outside.join("secret.fake"), "fn secret\n").unwrap();
    let mut index = indexed(&root);

    // A file replaced by a symlink pointing out of the project.
    let path = root.join("src/util.fake");
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(outside.join("secret.fake"), &path).unwrap();
    index.reindex_paths(&registry(), &[path]).unwrap();
    assert!(index.find_symbol("secret").unwrap().is_empty());
    assert!(index.find_symbol("helper").unwrap().is_empty());
    assert_matches_full(&index, &root);
}

#[test]
fn a_rename_reported_by_its_new_path_only_still_drops_the_old_one() {
    // What macOS FSEvents delivers through the debouncer's file-id cache.
    let root = project();
    let mut index = indexed(&root);

    // Same directory, content unchanged.
    fs::rename(root.join("src/util.fake"), root.join("src/helpers.fake")).unwrap();
    let report = index
        .reindex_paths(&registry(), &[root.join("src/helpers.fake")])
        .unwrap();
    assert_eq!((report.files_parsed, report.files_removed), (1, 1));
    assert_eq!(
        index.find_symbol("helper").unwrap()[0].relative_path,
        "src/helpers.fake"
    );
    assert_matches_full(&index, &root);

    // Same directory, edited within the same settled batch (found as a
    // sibling, the content hash no longer matches).
    fs::rename(root.join("src/helpers.fake"), root.join("src/renamed.fake")).unwrap();
    write(&root, "src/renamed.fake", "fn helper\nfn edited\n");
    index
        .reindex_paths(&registry(), &[root.join("src/renamed.fake")])
        .unwrap();
    assert_eq!(index.find_symbol("helper").unwrap().len(), 1);
    assert_matches_full(&index, &root);

    // Another directory, content unchanged (found by its content hash).
    fs::rename(root.join("src/renamed.fake"), root.join("lib/moved.fake")).unwrap();
    index
        .reindex_paths(&registry(), &[root.join("lib/moved.fake")])
        .unwrap();
    assert_eq!(
        index.find_symbol("helper").unwrap()[0].relative_path,
        "lib/moved.fake"
    );
    assert_matches_full(&index, &root);

    // A whole directory, reported by its new name only.
    fs::rename(root.join("lib"), root.join("pkg")).unwrap();
    index
        .reindex_paths(&registry(), &[root.join("pkg")])
        .unwrap();
    assert_eq!(index.find_symbol("extra").unwrap().len(), 1);
    assert_matches_full(&index, &root);
}

#[test]
fn a_moved_and_edited_file_reported_by_its_new_path_only_waits_for_a_full_reindex() {
    // The documented limit of checking only likely old ends: neither the
    // directory nor the content hash leads back to the old path.
    let root = project();
    let mut index = indexed(&root);
    fs::remove_file(root.join("src/util.fake")).unwrap();
    let moved = write(&root, "lib/util.fake", "fn helper\nfn edited\n");
    index.reindex_paths(&registry(), &[moved]).unwrap();
    assert_eq!(index.find_symbol("helper").unwrap().len(), 2);

    index.reindex(&registry(), false).unwrap();
    assert_matches_full(&index, &root);
}

#[test]
fn relative_paths_and_duplicates_are_accepted() {
    let root = project();
    let mut index = indexed(&root);
    write(&root, "src/util.fake", "fn helper\nfn again\n");
    let report = index
        .reindex_paths(
            &registry(),
            &[PathBuf::from("src/util.fake"), root.join("src/util.fake")],
        )
        .unwrap();
    assert_eq!(report.files_parsed, 1);
    assert_eq!(index.find_symbol("again").unwrap().len(), 1);
}

#[test]
fn the_root_itself_falls_back_to_a_full_reindex() {
    let root = project();
    let mut index = indexed(&root);
    fs::remove_file(root.join("lib/extra.fake")).unwrap();
    write(&root, "src/new.fake", "fn fresh\n");
    let report = index
        .reindex_paths(&registry(), std::slice::from_ref(&root))
        .unwrap();
    assert_eq!((report.files_parsed, report.files_removed), (1, 1));
    assert_matches_full(&index, &root);
}

#[test]
fn only_the_given_paths_are_looked_at() {
    let root = project();
    let mut index = indexed(&root);
    // Two files change on disk, only one is reported: the other stays as it
    // was indexed (a full reindex is what would pick it up).
    let reported = write(&root, "src/util.fake", "fn helper\nfn reported\n");
    write(&root, "lib/extra.fake", "fn not_reported\n");
    let report = index.reindex_paths(&registry(), &[reported]).unwrap();
    assert_eq!(report.files_parsed, 1);
    assert_eq!(index.find_symbol("reported").unwrap().len(), 1);
    assert!(index.find_symbol("not_reported").unwrap().is_empty());
    assert_eq!(index.find_symbol("extra").unwrap().len(), 1);
}

#[test]
fn valid_then_broken_then_valid_again_never_keeps_stale_symbols() {
    let root = project();
    let mut index = indexed(&root);
    // A second index kept up to date by full non-forced reindexes, the
    // other path through the same per-file rules.
    let mut full = indexed(&root);
    let path = root.join("src/util.fake");
    let original = fs::read_to_string(&path).unwrap();

    // Valid → broken: the previous version's symbols must go, the error
    // must be recorded.
    fs::write(&path, "fn helper\nthis is not fn\n").unwrap();
    index
        .reindex_paths(&registry(), std::slice::from_ref(&path))
        .unwrap();
    full.reindex(&registry(), false).unwrap();
    assert!(index.find_symbol("helper").unwrap().is_empty());
    assert!(index.find_symbol("unused").unwrap().is_empty());
    assert_eq!(index.status().unwrap().syntax_errors.len(), 1);
    assert_matches_full(&index, &root);
    assert_matches_full(&full, &root);

    // Broken → reverted to the exact content last indexed (same hash as
    // before the break): re-parsed, error cleared.
    fs::write(&path, &original).unwrap();
    index
        .reindex_paths(&registry(), std::slice::from_ref(&path))
        .unwrap();
    full.reindex(&registry(), false).unwrap();
    assert_eq!(index.find_symbol("helper").unwrap().len(), 1);
    assert!(index.status().unwrap().syntax_errors.is_empty());
    assert_matches_full(&index, &root);
    assert_matches_full(&full, &root);

    // Broken → fixed with new content.
    fs::write(&path, "fn broken(\n").unwrap();
    index
        .reindex_paths(&registry(), std::slice::from_ref(&path))
        .unwrap();
    fs::write(&path, "fn helper\nfn fixed\n").unwrap();
    index
        .reindex_paths(&registry(), std::slice::from_ref(&path))
        .unwrap();
    assert_eq!(index.find_symbol("fixed").unwrap().len(), 1);
    assert!(index.status().unwrap().syntax_errors.is_empty());
    assert_matches_full(&index, &root);
}

#[test]
fn a_file_turned_binary_drops_its_symbols() {
    let root = project();
    let mut index = indexed(&root);
    let path = root.join("src/util.fake");
    fs::write(&path, [0xff, 0xfe, 0x00, 0x66]).unwrap();
    index
        .reindex_paths(&registry(), std::slice::from_ref(&path))
        .unwrap();
    assert!(index.find_symbol("helper").unwrap().is_empty());
    assert_matches_full(&index, &root);
}

#[test]
fn an_edit_or_an_unrelated_create_checks_no_other_indexed_path() {
    let root = project();
    let mut index = indexed(&root);
    // Deleted without any event, then an unrelated edit is reported: the
    // edit alone doesn't pay for checking every indexed path.
    fs::remove_file(root.join("lib/extra.fake")).unwrap();
    let edited = write(&root, "src/util.fake", "fn helper\nfn edited\n");
    let report = index.reindex_paths(&registry(), &[edited]).unwrap();
    assert_eq!((report.files_parsed, report.files_removed), (1, 0));
    assert_eq!(index.find_symbol("extra").unwrap().len(), 1);

    // Neither does a create elsewhere: only a path's likely old ends are
    // checked. An event lost entirely is the full reindex's to catch.
    let created = write(&root, "src/new.fake", "fn fresh\n");
    let report = index.reindex_paths(&registry(), &[created]).unwrap();
    assert_eq!(report.files_removed, 0);
    index.reindex(&registry(), false).unwrap();
    assert_matches_full(&index, &root);
}

/// Whether `dir` sits on a case-insensitive filesystem (APFS/HFS+ and NTFS
/// by default).
fn case_insensitive(dir: &Path) -> bool {
    let probe = dir.join("CaseProbe");
    fs::write(&probe, "").unwrap();
    let insensitive = dir.join("caseprobe").exists();
    fs::remove_file(probe).unwrap();
    insensitive
}

#[test]
fn a_case_only_rename_moves_its_symbols_without_duplicates() {
    let root = project();
    if !case_insensitive(&root) {
        return; // the old spelling simply vanishes: the plain rename test
    }
    let mut index = indexed(&root);

    // Both ends reported (the old one still resolves, to the new spelling).
    let (from, to) = (root.join("src/util.fake"), root.join("src/Util.fake"));
    fs::rename(&from, &to).unwrap();
    index.reindex_paths(&registry(), &[from, to]).unwrap();
    let hits = index.find_symbol("helper").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].relative_path, "src/Util.fake");
    assert_matches_full(&index, &root);

    // Only the new end reported (macOS FSEvents through the debouncer).
    let (from, to) = (root.join("src/Util.fake"), root.join("src/UTIL.fake"));
    fs::rename(&from, &to).unwrap();
    index.reindex_paths(&registry(), &[to]).unwrap();
    let hits = index.find_symbol("helper").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].relative_path, "src/UTIL.fake");
    assert_matches_full(&index, &root);

    // A directory, new end only.
    fs::rename(root.join("lib"), root.join("Lib")).unwrap();
    index
        .reindex_paths(&registry(), &[root.join("Lib")])
        .unwrap();
    let hits = index.find_symbol("extra").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].relative_path, "Lib/extra.fake");
    assert_matches_full(&index, &root);
}

#[cfg(unix)]
#[test]
fn a_symlinked_directory_is_never_walked() {
    let root = project();
    let outside = tempdir();
    fs::write(outside.join("secret.fake"), "fn secret\n").unwrap();
    let mut index = indexed(&root);

    // Pointing out of the root, and back into it (would duplicate `src/`).
    let out_link = root.join("outside");
    let in_link = root.join("alias");
    std::os::unix::fs::symlink(&outside, &out_link).unwrap();
    std::os::unix::fs::symlink(root.join("src"), &in_link).unwrap();
    let report = index
        .reindex_paths(&registry(), &[out_link, in_link])
        .unwrap();
    assert_eq!((report.files_parsed, report.files_unchanged), (0, 0));
    assert!(index.find_symbol("secret").unwrap().is_empty());
    assert_eq!(index.find_symbol("helper").unwrap().len(), 1);
    assert_matches_full(&index, &root);
}
