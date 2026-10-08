//! Read failures (issue R04): a file, manifest or directory that stops being
//! readable *after* it was indexed is reported as its own issue — neither a
//! syntax error nor an unsupported language — and its last-good rows stay
//! served instead of being erased or silently going stale.
//!
//! Platform limits, all handled by skipping rather than failing:
//!
//! - The tests drive real Unix permission bits (`chmod 000`), so the whole file
//!   is `#[cfg(unix)]`. Windows ACLs have no equivalent a test can set portably.
//! - A process that bypasses permission checks (root, or a capability such as
//!   `CAP_DAC_OVERRIDE`) can read a `chmod 000` file. Each test first probes
//!   whether the denial is enforced for this process ([`denied`]) and returns
//!   early, printing why, when it is not. Running the suite as root therefore
//!   passes without exercising these cases; run it unprivileged to cover them.
//! - Some network or FUSE filesystems ignore mode bits; the same probe skips.
#![cfg(unix)]
// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::{Location, ParseError, ParsedFile, SourceFile, SymbolKind, SymbolRecord};
use mct_index::{ExcludeSet, Index, UnsupportedKind};

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
            let Some(name) = line.strip_prefix("fn ") else {
                return Err(ParseError::Syntax {
                    path: file.relative_path.clone(),
                    line: line_no as u32 + 1,
                    message: format!("expected `fn`, got `{line}`"),
                });
            };
            let id = parsed.symbols.len() as u32;
            parsed.symbols.push(SymbolRecord {
                id,
                name: name.trim().to_string(),
                kind: SymbolKind::Function,
                location: Location {
                    line: line_no as u32 + 1,
                    column: 1,
                    byte_len: line.len() as u32,
                    end_line: Some(line_no as u32 + 1),
                },
                parent: None,
                level: None,
            });
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
        "mct-read-failures-{}-{}",
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

fn chmod(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

/// Restores a path's permissions when dropped, so a failing assertion never
/// leaves an unreadable entry behind for the temp-dir cleaner.
struct Restore(PathBuf, u32);

impl Drop for Restore {
    fn drop(&mut self) {
        // The test may have removed the path itself: restoring is best-effort,
        // and a panic here would mask the test's real outcome.
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(self.1));
    }
}

/// Removes read access from `path` (`0o000`) and reports whether the denial is
/// actually enforced for this process. `None` means it is not (root, a
/// capability, or a filesystem ignoring mode bits): the caller must skip.
fn denied(path: &Path, original: u32) -> Option<Restore> {
    let guard = Restore(path.to_path_buf(), original);
    chmod(path, 0o000);
    let enforced = if path.is_dir() {
        fs::read_dir(path).is_err()
    } else {
        fs::read(path).is_err()
    };
    if enforced {
        Some(guard)
    } else {
        eprintln!(
            "skipping: permission denial on {} is not enforced for this process \
             (running as root, or a filesystem that ignores mode bits)",
            path.display()
        );
        None
    }
}

fn open(root: &Path) -> Index {
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry(), false).unwrap();
    index
}

fn names(index: &Index, name: &str) -> usize {
    index.find_symbol(name).unwrap().len()
}

#[test]
fn an_unreadable_file_keeps_its_symbols_and_is_flagged_not_a_syntax_error() {
    let root = tempdir();
    let file = write(&root, "a.fake", "fn alpha\n");
    write(&root, "b.fake", "fn beta\n");
    let mut index = open(&root);

    let Some(_guard) = denied(&file, 0o644) else {
        return;
    };
    let report = index.reindex(&registry(), false).unwrap();

    assert_eq!(names(&index, "alpha"), 1, "last-good symbols must stay");
    assert_eq!(names(&index, "beta"), 1);
    let status = index.status().unwrap();
    assert_eq!(status.total_files, 2);
    assert_eq!(status.read_failures.len(), 1, "{status:?}");
    assert_eq!(status.read_failures[0].relative_path, "a.fake");
    assert_eq!(status.read_failures[0].kind, UnsupportedKind::ReadFailure);
    assert!(status.syntax_errors.is_empty(), "{status:?}");
    assert!(status.unsupported_languages.is_empty(), "{status:?}");
    assert!(report
        .issues
        .iter()
        .any(|i| i.kind == UnsupportedKind::ReadFailure && i.relative_path == "a.fake"));
}

#[test]
fn a_read_failure_clears_once_the_file_is_readable_and_changes_are_picked_up() {
    let root = tempdir();
    let file = write(&root, "a.fake", "fn alpha\n");
    let mut index = open(&root);

    {
        let Some(_guard) = denied(&file, 0o644) else {
            return;
        };
        index.reindex(&registry(), false).unwrap();
        assert_eq!(index.status().unwrap().read_failures.len(), 1);
    }
    // Edited while it was unreadable (as far as the index knows).
    fs::write(&file, "fn gamma\n").unwrap();
    index.reindex(&registry(), false).unwrap();

    assert!(index.status().unwrap().read_failures.is_empty());
    assert_eq!(
        names(&index, "alpha"),
        0,
        "the new content replaces the old"
    );
    assert_eq!(names(&index, "gamma"), 1);
}

#[test]
fn an_unreadable_then_unchanged_file_also_clears_on_the_hash_skip_path() {
    let root = tempdir();
    let file = write(&root, "a.fake", "fn alpha\n");
    let mut index = open(&root);

    {
        let Some(_guard) = denied(&file, 0o644) else {
            return;
        };
        index.reindex(&registry(), false).unwrap();
    }
    let report = index.reindex(&registry(), false).unwrap();

    assert_eq!(report.files_unchanged, 1);
    assert!(index.status().unwrap().read_failures.is_empty());
    assert_eq!(names(&index, "alpha"), 1);
}

#[test]
fn deleting_a_file_still_drops_it_while_another_is_unreadable() {
    let root = tempdir();
    let kept = write(&root, "a.fake", "fn alpha\n");
    let gone = write(&root, "b.fake", "fn beta\n");
    let mut index = open(&root);

    let Some(_guard) = denied(&kept, 0o644) else {
        return;
    };
    fs::remove_file(&gone).unwrap();
    let report = index.reindex(&registry(), false).unwrap();

    assert_eq!(report.files_removed, 1);
    assert_eq!(names(&index, "beta"), 0);
    assert_eq!(names(&index, "alpha"), 1);
}

#[test]
fn deleting_an_unreadable_file_drops_its_rows_and_its_failure() {
    let root = tempdir();
    let file = write(&root, "sub/a.fake", "fn alpha\n");
    let mut index = open(&root);

    let Some(_guard) = denied(&file, 0o644) else {
        return;
    };
    index.reindex(&registry(), false).unwrap();
    assert_eq!(index.status().unwrap().read_failures.len(), 1);

    // Removing a file needs write access to its directory, not to the file.
    fs::remove_file(&file).unwrap();
    index.reindex(&registry(), false).unwrap();

    assert_eq!(names(&index, "alpha"), 0);
    assert!(index.status().unwrap().read_failures.is_empty());
}

#[test]
fn an_unreadable_directory_keeps_everything_under_it() {
    let root = tempdir();
    write(&root, "pkg/inner/a.fake", "fn alpha\n");
    write(&root, "pkg/b.fake", "fn beta\n");
    write(&root, "top.fake", "fn top\n");
    let mut index = open(&root);

    let dir = root.join("pkg/inner");
    let Some(_guard) = denied(&dir, 0o755) else {
        return;
    };
    let report = index.reindex(&registry(), false).unwrap();

    assert_eq!(report.files_removed, 0, "{report:?}");
    assert_eq!(names(&index, "alpha"), 1, "unreadable is not deleted");
    assert_eq!(names(&index, "beta"), 1);
    assert_eq!(names(&index, "top"), 1);
    let status = index.status().unwrap();
    assert_eq!(status.total_files, 3);
    assert_eq!(status.read_failures.len(), 1, "{status:?}");
    assert_eq!(status.read_failures[0].relative_path, "pkg/inner");
}

#[test]
fn a_directory_failure_clears_when_it_is_readable_again() {
    let root = tempdir();
    write(&root, "pkg/a.fake", "fn alpha\n");
    let mut index = open(&root);

    {
        let Some(_guard) = denied(&root.join("pkg"), 0o755) else {
            return;
        };
        index.reindex(&registry(), false).unwrap();
        assert_eq!(index.status().unwrap().read_failures.len(), 1);
    }
    index.reindex(&registry(), false).unwrap();

    assert!(index.status().unwrap().read_failures.is_empty());
    assert_eq!(names(&index, "alpha"), 1);
}

#[test]
fn a_file_deleted_inside_a_now_readable_directory_is_dropped_after_the_failure() {
    let root = tempdir();
    write(&root, "pkg/a.fake", "fn alpha\n");
    write(&root, "pkg/b.fake", "fn beta\n");
    let mut index = open(&root);

    {
        let Some(_guard) = denied(&root.join("pkg"), 0o755) else {
            return;
        };
        index.reindex(&registry(), false).unwrap();
        assert_eq!(names(&index, "alpha"), 1);
    }
    fs::remove_file(root.join("pkg/b.fake")).unwrap();
    index.reindex(&registry(), false).unwrap();

    assert_eq!(names(&index, "beta"), 0);
    assert_eq!(names(&index, "alpha"), 1);
}

#[test]
fn an_unreadable_manifest_keeps_its_dependencies_and_is_flagged() {
    let root = tempdir();
    let manifest = write(&root, "Cargo.toml", "[dependencies]\nserde = \"1\"\n");
    let mut index = open(&root);
    assert_eq!(index.status().unwrap().dependencies.len(), 1);

    let Some(_guard) = denied(&manifest, 0o644) else {
        return;
    };
    index.reindex(&registry(), false).unwrap();

    let status = index.status().unwrap();
    assert_eq!(status.dependencies.len(), 1, "{status:?}");
    assert_eq!(status.dependencies[0].dependencies[0].name, "serde");
    assert_eq!(status.read_failures.len(), 1, "{status:?}");
    assert_eq!(status.read_failures[0].relative_path, "Cargo.toml");
    assert_eq!(status.read_failures[0].kind, UnsupportedKind::ReadFailure);
}

#[test]
fn a_manifest_failure_clears_and_picks_up_the_new_dependencies() {
    let root = tempdir();
    let manifest = write(&root, "Cargo.toml", "[dependencies]\nserde = \"1\"\n");
    let mut index = open(&root);

    {
        let Some(_guard) = denied(&manifest, 0o644) else {
            return;
        };
        index.reindex(&registry(), false).unwrap();
    }
    fs::write(&manifest, "[dependencies]\nanyhow = \"1\"\n").unwrap();
    index.reindex(&registry(), false).unwrap();

    let status = index.status().unwrap();
    assert!(status.read_failures.is_empty(), "{status:?}");
    assert_eq!(status.dependencies[0].dependencies[0].name, "anyhow");
}

#[test]
fn incremental_updates_keep_last_good_rows_and_clear_the_failure() {
    let root = tempdir();
    let file = write(&root, "a.fake", "fn alpha\n");
    let mut index = open(&root);

    {
        let Some(_guard) = denied(&file, 0o644) else {
            return;
        };
        let report = index
            .reindex_paths(&registry(), std::slice::from_ref(&file))
            .unwrap();
        assert!(report
            .issues
            .iter()
            .any(|i| i.kind == UnsupportedKind::ReadFailure));
        assert_eq!(names(&index, "alpha"), 1, "last-good symbols stay");
        assert_eq!(index.status().unwrap().read_failures.len(), 1);
    }
    fs::write(&file, "fn gamma\n").unwrap();
    index
        .reindex_paths(&registry(), std::slice::from_ref(&file))
        .unwrap();

    assert!(index.status().unwrap().read_failures.is_empty());
    assert_eq!(names(&index, "alpha"), 0);
    assert_eq!(names(&index, "gamma"), 1);
}

#[test]
fn an_incremental_directory_walk_retains_what_it_cannot_list() {
    let root = tempdir();
    write(&root, "pkg/inner/a.fake", "fn alpha\n");
    write(&root, "pkg/b.fake", "fn beta\n");
    let mut index = open(&root);

    let Some(_guard) = denied(&root.join("pkg/inner"), 0o755) else {
        return;
    };
    let pkg = root.join("pkg");
    let report = index
        .reindex_paths(&registry(), std::slice::from_ref(&pkg))
        .unwrap();

    assert_eq!(report.files_removed, 0, "{report:?}");
    assert_eq!(names(&index, "alpha"), 1);
    assert_eq!(names(&index, "beta"), 1);
    let status = index.status().unwrap();
    assert_eq!(status.read_failures.len(), 1, "{status:?}");
    assert_eq!(status.read_failures[0].relative_path, "pkg/inner");
}

#[test]
fn a_path_that_cannot_even_be_stat_ed_is_not_taken_for_deleted() {
    let root = tempdir();
    let file = write(&root, "pkg/a.fake", "fn alpha\n");
    let mut index = open(&root);

    // No search permission on the parent: `stat` of the child fails with
    // `PermissionDenied`, which is not the same as the child being gone.
    let Some(_guard) = denied(&root.join("pkg"), 0o755) else {
        return;
    };
    let report = index
        .reindex_paths(&registry(), std::slice::from_ref(&file))
        .unwrap();

    assert_eq!(report.files_removed, 0, "{report:?}");
    assert_eq!(names(&index, "alpha"), 1);
    let status = index.status().unwrap();
    assert_eq!(status.read_failures.len(), 1, "{status:?}");
    assert_eq!(status.read_failures[0].relative_path, "pkg/a.fake");
}

#[test]
fn a_genuine_syntax_error_is_still_a_syntax_error_not_a_read_failure() {
    let root = tempdir();
    write(&root, "a.fake", "fn alpha\n");
    let mut index = open(&root);

    write(&root, "a.fake", "this is not valid\n");
    index.reindex(&registry(), false).unwrap();

    let status = index.status().unwrap();
    assert_eq!(status.syntax_errors.len(), 1, "{status:?}");
    assert!(status.read_failures.is_empty(), "{status:?}");
}
