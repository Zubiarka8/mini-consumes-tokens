//! `compare-index` (issue #637): two indexes of one project compared by
//! file path, symbol identity and relation key, never by row id, and
//! without writing either database.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

#[path = "common/command.rs"]
mod command;
use command::mct;

fn write(root: &Path, path: &str, contents: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn init(root: &Path) {
    let output = mct(&[&"--root", &root, &"init", &"--no-obsidian"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn two_indexes_are_compared_by_identity_not_row_id() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "src/lib.rs",
        "pub fn run() {\n    helper();\n}\npub fn old() {}\n",
    );
    write(
        root,
        "src/a.rs",
        "pub fn helper() {}\npub fn grow(a: u8) {}\n",
    );
    write(root, "src/b.rs", "pub fn helper() {}\n");
    init(root);
    let db = root.join(".mct-index/index.sqlite3");
    let before = dir.path().join("before.sqlite3");
    // A fresh index renumbers every row: identity must not depend on ids.
    fs::copy(&db, &before).unwrap();
    fs::remove_dir_all(root.join(".mct-index")).unwrap();

    write(
        root,
        "src/lib.rs",
        "pub fn run() {\n    helper();\n}\npub fn fresh() {}\n",
    );
    write(
        root,
        "src/a.rs",
        "pub fn helper() {}\npub fn grow(a: u8, b: u8) {}\n",
    );
    fs::remove_file(root.join("src/b.rs")).unwrap();
    init(root);
    let after_bytes = fs::read(&db).unwrap();

    let output = mct(&[&"--root", &root, &"compare-index", &"--before", &before]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(report.contains("Files: +0 -1 (2 unchanged)\n"), "{report}");
    assert!(
        report.contains("  2 file(s) on both sides with new content\n"),
        "{report}"
    );
    assert!(
        report.contains("  1 symbol(s) on both sides with a new signature\n"),
        "{report}"
    );
    assert!(
        report.contains(
            "Signatures changed:\n  src/a.rs function grow\n    \
             before: pub fn grow(a: u8)\n    \
             after:  pub fn grow(a: u8, b: u8)\n"
        ),
        "{report}"
    );
    assert!(
        report
            .contains("Symbols added:\n  src/a.rs variable grow.b\n  src/lib.rs function fresh\n"),
        "{report}"
    );
    assert!(report.contains("  src/b.rs function helper\n"), "{report}");
    assert!(report.contains("  src/lib.rs function old\n"), "{report}");
    assert!(report.contains("  ambiguous -> resolved: 1\n"), "{report}");
    assert!(
        report.contains(
            "  src/lib.rs function run calls helper\n    \
             before: ambiguous src/a.rs:helper|src/b.rs:helper\n    \
             after:  resolved src/a.rs:helper\n"
        ),
        "{report}"
    );
    // Read-only: neither database changed.
    assert_eq!(fs::read(&db).unwrap(), after_bytes);

    // Against itself, nothing changed.
    let same = mct(&[
        &"--root",
        &root,
        &"compare-index",
        &"--before",
        &db,
        &"--after",
        &db,
    ]);
    let same = String::from_utf8_lossy(&same.stdout).into_owned();
    assert!(
        same.contains("Relations decided differently: 0\n"),
        "{same}"
    );
    assert!(same.contains("Symbols: +0 -0"), "{same}");
    assert!(
        same.contains("  0 symbol(s) on both sides with a new signature\n"),
        "{same}"
    );
}

#[test]
fn a_missing_index_is_an_error_not_a_new_database() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.sqlite3");
    let output = mct(&[
        &"--root",
        &dir.path(),
        &"compare-index",
        &"--before",
        &missing,
    ]);
    assert!(!output.status.success());
    assert!(!missing.exists());
}
