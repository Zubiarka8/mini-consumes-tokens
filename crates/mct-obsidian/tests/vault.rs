#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use mct_index::{ExcludeSet, Index};
use mct_obsidian::{vault_dir, write_vault, MANIFEST_FILE};

fn project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mct-obsidian-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn open(dir: &Path) -> Index {
    Index::open_in_memory(dir, ExcludeSet::default()).unwrap()
}

fn note_path(out: &Path, source: &str) -> PathBuf {
    mct_obsidian::Manifest::load(out)
        .unwrap()
        .notes
        .keys()
        .filter(|p| p.starts_with("files/"))
        .find(|p| {
            fs::read_to_string(out.join(p))
                .unwrap()
                .contains(&format!("path: \"{source}\""))
        })
        .map(|p| out.join(p))
        .unwrap()
}

fn reindex(index: &mut Index) -> mct_index::ReindexReport {
    index
        .reindex(&mct_languages::build_registry(), false)
        .unwrap()
}

#[test]
fn a_vault_contains_a_root_index_file_notes_and_a_manifest() {
    let dir = project("basic");
    fs::write(dir.join("lib.rs"), "pub fn add() {}\n").unwrap();
    fs::write(dir.join("empty.rs"), "\n").unwrap();
    let mut index = open(&dir);
    reindex(&mut index);

    let out = vault_dir(index.root());
    let report = write_vault(&index, &out).unwrap();
    assert_eq!(report.notes_written, 3);

    let note = fs::read_to_string(out.join("index.md")).unwrap();
    assert!(note.contains("[lib.rs](files/"), "{note}");
    let source_note = fs::read_to_string(note_path(&out, "lib.rs")).unwrap();
    assert!(source_note.contains("path: \"lib.rs\""), "{source_note}");
    assert!(
        source_note.contains("**`function`** `add` · line 1"),
        "{source_note}"
    );
    assert!(out.join(MANIFEST_FILE).is_file());
    assert!(note_path(&out, "empty.rs").is_file());
    assert!(!out.with_file_name("obsidian.tmp").exists());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn nothing_of_the_project_is_copied() {
    let dir = project("nocopy");
    fs::write(dir.join("notes.md"), "# Title\n\nUNIQUE-BODY-TEXT\n").unwrap();
    fs::write(
        dir.join("a.rs"),
        "pub fn a() { let _ = \"UNIQUE-BODY-TEXT\"; }\n",
    )
    .unwrap();
    let mut index = open(&dir);
    reindex(&mut index);
    let out = vault_dir(index.root());
    write_vault(&index, &out).unwrap();

    let mut files = Vec::new();
    collect(&out, &out, &mut files);
    files.sort();
    assert_eq!(files.len(), 4);
    assert!(note_path(&out, "a.rs").is_file());
    assert!(note_path(&out, "notes.md").is_file());
    for path in &files {
        assert!(!fs::read_to_string(out.join(path))
            .unwrap()
            .contains("UNIQUE-BODY-TEXT"));
    }
    assert!(!fs::read_to_string(out.join("index.md"))
        .unwrap()
        .contains("UNIQUE-BODY-TEXT"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_unchanged_index_writes_nothing() {
    let dir = project("noop");
    fs::write(dir.join("lib.rs"), "pub fn add() {}\n").unwrap();
    let mut index = open(&dir);
    reindex(&mut index);
    let out = vault_dir(index.root());
    write_vault(&index, &out).unwrap();

    let again = write_vault(&index, &out).unwrap();
    assert_eq!((again.notes_written, again.notes_removed), (0, 0));
    assert_eq!(again.notes_unchanged, 2);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn editing_a_file_updates_the_index_note_and_deleting_one_drops_it() {
    let dir = project("edit");
    fs::write(dir.join("a.rs"), "pub fn a() {}\n").unwrap();
    fs::write(dir.join("b.rs"), "pub fn b() {}\n").unwrap();
    let mut index = open(&dir);
    reindex(&mut index);
    let out = vault_dir(index.root());
    write_vault(&index, &out).unwrap();

    let deleted = note_path(&out, "b.rs");
    fs::write(dir.join("a.rs"), "pub fn a() {}\npub fn a2() {}\n").unwrap();
    fs::remove_file(dir.join("b.rs")).unwrap();
    reindex(&mut index);
    let report = write_vault(&index, &out).unwrap();
    assert_eq!(report.notes_written, 2, "{report:?}");

    let note = fs::read_to_string(note_path(&out, "a.rs")).unwrap();
    assert!(note.contains("`a2` · line 2"), "{note}");
    assert!(!deleted.exists());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn stale_notes_from_an_older_manifest_are_cleaned_up() {
    let dir = project("legacy");
    fs::write(dir.join("a.rs"), "pub fn a() {}\n").unwrap();
    let mut index = open(&dir);
    reindex(&mut index);
    let out = vault_dir(index.root());
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("old-layout.md"), "old").unwrap();
    fs::write(
        out.join(MANIFEST_FILE),
        format!(
            "notes[1]{{hash,path}}:\n  {},old-layout.md\n",
            mct_obsidian::hash_note("old")
        ),
    )
    .unwrap();

    write_vault(&index, &out).unwrap();
    assert!(!out.join("old-layout.md").exists());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn obsidians_own_config_folder_survives_regeneration() {
    let dir = project("config");
    fs::write(dir.join("a.rs"), "pub fn a() {}\n").unwrap();
    let mut index = open(&dir);
    reindex(&mut index);
    let out = vault_dir(index.root());
    write_vault(&index, &out).unwrap();
    fs::create_dir_all(out.join(".obsidian")).unwrap();
    fs::write(out.join(".obsidian/app.json"), "{}").unwrap();

    fs::write(dir.join("a.rs"), "pub fn changed() {}\n").unwrap();
    reindex(&mut index);
    write_vault(&index, &out).unwrap();
    assert!(out.join(".obsidian/app.json").is_file());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn imports_are_listed_under_the_importing_file() {
    let dir = project("imports");
    fs::write(dir.join("util.py"), "def helper():\n    pass\n").unwrap();
    fs::write(
        dir.join("app.py"),
        "import util\n\ndef run():\n    util.helper()\n",
    )
    .unwrap();
    let mut index = open(&dir);
    reindex(&mut index);
    let out = vault_dir(index.root());
    write_vault(&index, &out).unwrap();
    let note = fs::read_to_string(note_path(&out, "app.py")).unwrap();
    assert!(note.contains("[util.py](../files/"), "{note}");
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn a_symlinked_vault_directory_is_refused() {
    let dir = project("symlink");
    fs::write(dir.join("a.rs"), "pub fn a() {}\n").unwrap();
    let mut index = open(&dir);
    reindex(&mut index);
    let target = dir.join("elsewhere");
    fs::create_dir_all(&target).unwrap();
    let out = vault_dir(index.root());
    fs::create_dir_all(out.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, &out).unwrap();

    assert!(write_vault(&index, &out).is_err());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    fs::remove_dir_all(&dir).ok();
}

/// The markdown fixture vault rendered once and compared with the checked-in
/// snapshot. `MCT_BLESS=1 cargo test -p mct-obsidian` rewrites it.
#[test]
fn the_markdown_fixture_vault_matches_its_snapshot() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mct-lang-md/tests/fixtures/vault");
    let dir = project("snapshot");
    copy_dir(&fixture, &dir);
    let mut index = open(&dir);
    reindex(&mut index);
    let out = vault_dir(index.root());
    write_vault(&index, &out).unwrap();

    let rendered = fs::read_to_string(out.join("index.md")).unwrap();
    let snapshots = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    let expected = snapshots.join("markdown_vault.txt");
    if std::env::var_os("MCT_BLESS").is_some() {
        fs::create_dir_all(&snapshots).unwrap();
        fs::write(&expected, &rendered).unwrap();
    }
    // A Windows checkout may convert the snapshot to CRLF; the vault is always LF.
    let expected = fs::read_to_string(&expected).unwrap().replace("\r\n", "\n");
    assert_eq!(rendered, expected);
    fs::remove_dir_all(&dir).ok();
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&dest).unwrap();
            copy_dir(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), dest).unwrap();
        }
    }
}

fn collect(base: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(base, &path, out);
        } else {
            out.push(
                path.strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

#[test]
fn wide_maps_are_paged_and_every_generated_note_is_reachable_with_valid_links() {
    use std::collections::{BTreeMap, BTreeSet};
    let dir = project("wide");
    fs::create_dir_all(dir.join("src/nested")).unwrap();
    for i in 0..205 {
        fs::write(dir.join(format!("src/f{i}.rs")), "pub fn function() {}\n").unwrap();
    }
    fs::write(dir.join("src/nested/a.rs"), "\n").unwrap();
    let mut index = open(&dir);
    reindex(&mut index);
    let out = vault_dir(index.root());
    write_vault(&index, &out).unwrap();
    let manifest = mct_obsidian::Manifest::load(&out).unwrap();
    let mut edges = BTreeMap::<String, Vec<String>>::new();
    for path in manifest.notes.keys() {
        let text = fs::read_to_string(out.join(path)).unwrap();
        if path.starts_with("directories/") {
            assert!(text.lines().filter(|line| line.starts_with("- [")).count() <= 100);
        }
        for fragment in text.split("](").skip(1) {
            let target = fragment.split_once(')').unwrap().0;
            let resolved = out
                .join(path)
                .parent()
                .unwrap()
                .join(target)
                .canonicalize()
                .unwrap();
            assert!(resolved.is_file());
            let relative = resolved
                .strip_prefix(out.canonicalize().unwrap())
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            assert!(manifest.notes.contains_key(&relative), "{path}: {target}");
            edges.entry(path.clone()).or_default().push(relative);
        }
    }
    let mut todo = vec!["index.md".to_string()];
    let mut seen = BTreeSet::new();
    while let Some(path) = todo.pop() {
        if seen.insert(path.clone()) {
            todo.extend(edges.get(&path).into_iter().flatten().cloned());
        }
    }
    assert_eq!(seen.len(), manifest.notes.len());
    assert!(manifest.notes.keys().any(|p| p.ends_with("-page-3.md")));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn rename_updates_import_links_and_keeps_unrelated_note_mtime() {
    let dir = project("rename");
    fs::write(dir.join("util.py"), "def helper():\n    pass\n").unwrap();
    fs::write(dir.join("stable.py"), "def stable():\n    pass\n").unwrap();
    fs::write(dir.join("app.py"), "import util\n").unwrap();
    let mut index = open(&dir);
    mct_obsidian::register(&mut index);
    reindex(&mut index);
    let out = vault_dir(index.root());
    let old = note_path(&out, "util.py");
    let stable = note_path(&out, "stable.py");
    let modified = fs::metadata(&stable).unwrap().modified().unwrap();
    fs::rename(dir.join("util.py"), dir.join("renamed.py")).unwrap();
    fs::write(dir.join("app.py"), "import renamed\n").unwrap();
    let report = index
        .reindex_paths(
            &mct_languages::build_registry(),
            &[
                dir.join("util.py"),
                dir.join("renamed.py"),
                dir.join("app.py"),
            ],
        )
        .unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(!old.exists());
    let app = fs::read_to_string(note_path(&out, "app.py")).unwrap();
    assert!(app.contains("[renamed.py](../files/"), "{app}");
    assert!(!app.contains("util.py"));
    assert_eq!(fs::metadata(stable).unwrap().modified().unwrap(), modified);
    fs::remove_dir_all(&dir).ok();
}
