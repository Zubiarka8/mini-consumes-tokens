//! The generated atlas and SQLite use one root ignore policy.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run(root: &Path, command: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mct-cli"))
        .arg("--root")
        .arg(root)
        .arg(command)
        .output()
        .unwrap()
}
fn ok(root: &Path, command: &str) {
    let output = run(root, command);
    assert!(
        output.status.success(),
        "{command}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn paths(root: &Path) -> BTreeSet<String> {
    mct_index::Index::open_existing_read_only(
        root,
        &root.join(".mct-index/index.sqlite3"),
        mct_index::ExcludeSet::default(),
    )
    .unwrap()
    .indexed_files()
    .unwrap()
    .into_iter()
    .filter(|file| file.language != "asset")
    .map(|file| file.relative_path)
    .collect()
}
fn notes(root: &Path) -> BTreeMap<String, String> {
    let vault = mct_obsidian::vault_dir(root);
    mct_obsidian::Manifest::load(&vault)
        .unwrap()
        .notes
        .keys()
        .map(|path| (path.clone(), fs::read_to_string(vault.join(path)).unwrap()))
        .collect()
}
fn note_paths(root: &Path) -> BTreeSet<String> {
    notes(root)
        .iter()
        .filter(|(path, _)| path.starts_with("files/"))
        .map(|(_, text)| {
            let scalar = text
                .lines()
                .find_map(|line| line.strip_prefix("path: "))
                .unwrap();
            serde_json::from_str(scalar).unwrap()
        })
        .collect()
}
fn assert_shared(root: &Path) {
    assert_eq!(paths(root), note_paths(root));
}

#[test]
fn root_rules_imports_and_reincludes_select_the_same_files_for_database_and_atlas() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let rules = "venv/\nbin/\nobj/\n*.md\n!keep.md\n@import-gitignore\n";
    fs::write(root.join(".mctignore"), rules).unwrap();
    fs::write(root.join(".gitignore"), "third_party/\n").unwrap();
    for path in [
        "venv/noise.rs",
        "nested/bin/noise.rs",
        "nested/obj/noise.rs",
        "third_party/noise.rs",
    ] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "pub fn noise() {}\n").unwrap();
    }
    fs::write(root.join("skip.md"), "# Excluded\n").unwrap();
    fs::write(root.join("keep.md"), "# Included\n").unwrap();
    fs::write(root.join("app.py"), "import util\n").unwrap();
    fs::write(root.join("util.py"), "def helper():\n    pass\n").unwrap();
    ok(&root, "init");
    assert_shared(&root);
    assert_eq!(
        paths(&root),
        ["app.py", "keep.md", "util.py"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
    assert!(notes(&root)
        .values()
        .any(|text| text.contains("[util.py](../files/")));
    fs::write(root.join(".mctignore"), format!("{rules}util.py\n")).unwrap();
    // Manual generation only updates the projection; the following reindex
    // makes stored SQLite rows and derived notes equal again.
    ok(&root, "obsidian");
    assert!(paths(&root).contains("util.py"));
    assert!(!note_paths(&root).contains("util.py"));
    assert!(!notes(&root).values().any(|text| text.contains("util.py")));
    ok(&root, "reindex");
    assert_shared(&root);
    assert!(!paths(&root).contains("util.py"));
    fs::write(root.join(".mctignore"), rules).unwrap();
    ok(&root, "reindex");
    assert_shared(&root);
    assert!(paths(&root).contains("util.py"));
    assert!(notes(&root)
        .values()
        .any(|text| text.contains("[util.py](../files/")));
}

#[test]
fn unreadable_ignore_rules_preserve_the_last_vault_and_stop_indexing() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    fs::write(root.join("lib.rs"), "pub fn keep() {}\n").unwrap();
    fs::write(root.join(".mctignore"), "@import-gitignore\n").unwrap();
    fs::write(root.join(".gitignore"), "venv/\n").unwrap();
    ok(&root, "init");
    let previous = notes(&root);
    fs::write(root.join("new.rs"), "pub fn not_indexed_yet() {}\n").unwrap();
    for (bad_file, restore) in [
        (".mctignore", "@import-gitignore\n"),
        (".gitignore", "venv/\n"),
    ] {
        fs::write(root.join(bad_file), [0xff, 0xfe]).unwrap();
        for command in ["obsidian", "reindex", "init"] {
            let output = run(&root, command);
            assert!(
                !output.status.success(),
                "{command} must reject unreadable {bad_file}"
            );
            assert_eq!(
                notes(&root),
                previous,
                "{command} must preserve the valid vault"
            );
            assert!(!paths(&root).contains("new.rs"));
        }
        fs::write(root.join(bad_file), restore).unwrap();
    }
}
