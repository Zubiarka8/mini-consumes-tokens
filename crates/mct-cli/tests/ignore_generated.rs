//! Optional ignore preset through the real CLI and its existing index APIs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mct-cli"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn generated_preset_preserves_overrides_is_idempotent_and_excludes_at_any_depth() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let ignore = root.join(".mctignore");
    let original = "# User rules\ncustom/\n!src/bin/\n";
    fs::write(&ignore, original).unwrap();
    let output = run(&root, &["ignore-init", "--generated", "--import-gitignore"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let first = fs::read_to_string(&ignore).unwrap();
    assert!(first.contains(original) && first.contains("@import-gitignore"));
    assert!(!root.join(".mct-index").exists());
    assert!(
        run(&root, &["ignore-init", "--generated", "--import-gitignore"])
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(&ignore).unwrap(), first);
    let exclusions = mct_index::ExcludeSet::for_project(&root);
    for dir in [
        "venv",
        ".venv",
        "bin",
        "obj",
        "build",
        "dist",
        "__pycache__",
        ".tox",
        ".nox",
        ".mypy_cache",
        ".pytest_cache",
        ".ruff_cache",
    ] {
        for file in [
            format!("{dir}/generated.rs"),
            format!("nested/project/{dir}/generated.rs"),
        ] {
            assert!(exclusions.is_excluded(&file), "{file}");
            let path = root.join(&file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "pub fn excluded_noise() {}\n").unwrap();
        }
    }
    assert!(!exclusions.is_excluded("src/bin/tool.rs"));
    fs::create_dir_all(root.join("src/bin")).unwrap();
    fs::write(root.join("src/bin/tool.rs"), "pub fn useful_source() {}\n").unwrap();
    assert!(run(&root, &["init"]).status.success());
    let index = mct_index::Index::open_existing_read_only(
        &root,
        &root.join(".mct-index/index.sqlite3"),
        exclusions,
    )
    .unwrap();
    let files = index.indexed_files().unwrap();
    assert!(files
        .iter()
        .any(|file| file.relative_path == "src/bin/tool.rs"));
    assert!(!files
        .iter()
        .any(|file| file.relative_path.ends_with("generated.rs")));
    let notes = mct_obsidian::Manifest::load(&mct_obsidian::vault_dir(&root)).unwrap();
    for note in notes.notes.keys() {
        assert!(
            !fs::read_to_string(mct_obsidian::vault_dir(&root).join(note))
                .unwrap()
                .contains("excluded_noise")
        );
    }
}

#[test]
fn preset_is_opt_in_and_refuses_symlinked_configuration() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    assert!(run(&root, &["ignore-init"]).status.success());
    let file = root.join(".mctignore");
    assert!(!fs::read_to_string(&file)
        .unwrap()
        .lines()
        .any(|line| line == "bin/"));
    assert!(!mct_index::ExcludeSet::for_project(&root).is_excluded("src/bin/tool.rs"));
    #[cfg(unix)]
    {
        let outside = root.join("personal.txt");
        fs::write(&outside, "personal settings").unwrap();
        fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&outside, &file).unwrap();
        assert!(!run(&root, &["ignore-init", "--generated"]).status.success());
        assert_eq!(fs::read_to_string(outside).unwrap(), "personal settings");
    }
}

#[test]
fn activating_the_preset_cleans_previously_indexed_generated_files_on_reindex() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    for dir in ["venv", "nested/bin", "nested/obj", "src"] {
        fs::create_dir_all(root.join(dir)).unwrap();
        fs::write(
            root.join(dir).join("file.rs"),
            "pub fn indexed_function() {}\n",
        )
        .unwrap();
    }
    assert!(run(&root, &["init", "--no-obsidian"]).status.success());
    let read_paths = || {
        mct_index::Index::open_existing_read_only(
            &root,
            &root.join(".mct-index/index.sqlite3"),
            mct_index::ExcludeSet::default(),
        )
        .unwrap()
        .indexed_files()
        .unwrap()
        .into_iter()
        .map(|file| file.relative_path)
        .collect::<Vec<_>>()
    };
    assert!(read_paths().contains(&"nested/obj/file.rs".to_string()));
    assert!(run(&root, &["ignore-init", "--generated"]).status.success());
    assert!(run(&root, &["reindex"]).status.success());
    assert_eq!(read_paths(), vec!["src/file.rs".to_string()]);
    assert!(mct_obsidian::vault_dir(&root).join("index.md").is_file());
}
