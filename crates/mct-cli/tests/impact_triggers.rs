#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::Output;

use git2::{IndexAddOption, Repository, Signature};
use tempfile::TempDir;

struct Project {
    directory: TempDir,
}

impl Project {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let project = Self { directory };
        project.write(
            "Cargo.toml",
            "[package]\nname = \"impact_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        );
        project.write(
            "src/lib.rs",
            "pub fn parse_item() { validate(); }\nfn validate() {}\n",
        );
        project.write(
            "tests/parse_item.rs",
            "#[test]\nfn test_parse_item() { impact_fixture::parse_item(); }\n",
        );
        project.commit("base");
        project
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }

    fn write(&self, path: &str, contents: &str) {
        let path = self.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn commit(&self, message: &str) -> String {
        let repository = Repository::open(self.path())
            .unwrap_or_else(|_| Repository::init(self.path()).unwrap());
        let mut index = repository.index().unwrap();
        index
            .add_all(
                ["Cargo.toml", "src", "tests", "nested"],
                IndexAddOption::DEFAULT,
                None,
            )
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        let signature = Signature::now("Test", "test@example.com").unwrap();
        let parent = repository
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<_> = parent.iter().collect();
        let commit = repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parents,
            )
            .unwrap();
        commit.to_string()
    }
}

#[path = "common/command.rs"]
mod command;
use command::mct;

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn reindex_summary_is_opt_in_and_names_affected_tests() {
    let project = Project::new();
    let init = mct(&[&"--root", &project.path(), &"init"]);
    assert!(init.status.success(), "{}", stderr(&init));

    project.write(
        "src/lib.rs",
        "pub fn parse_item() { validate(); normalize(); }\nfn validate() {}\nfn normalize() {}\n",
    );
    let output = mct(&[&"--root", &project.path(), &"reindex", &"--summary"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let report = stdout(&output);
    assert!(report.contains("src/lib.rs"), "{report}");
    assert!(report.contains("parse_item"), "{report}");
    assert!(report.contains("test_parse_item"), "{report}");
}

#[test]
fn reindex_summary_includes_deleted_file_paths() {
    let project = Project::new();
    let init = mct(&[&"--root", &project.path(), &"init"]);
    assert!(init.status.success(), "{}", stderr(&init));

    fs::remove_file(project.path().join("src/lib.rs")).unwrap();
    let output = mct(&[&"--root", &project.path(), &"reindex", &"--summary"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("src/lib.rs"));
}

#[test]
fn impact_command_compares_commit_trees_and_names_affected_tests() {
    let project = Project::new();
    let base = project.commit("unchanged base");
    project.write(
        "src/lib.rs",
        "pub fn parse_item() { validate(); normalize(); }\nfn validate() {}\nfn normalize() {}\n",
    );
    let head = project.commit("change parser");

    let output = mct(&[
        &"--root",
        &project.path(),
        &"impact",
        &"--base",
        &base,
        &"--head",
        &head,
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let report = stdout(&output);
    assert!(report.contains("src/lib.rs"), "{report}");
    assert!(report.contains("parse_item"), "{report}");
    assert!(report.contains("test_parse_item"), "{report}");
}

#[test]
fn impact_command_scopes_git_paths_to_a_nested_project_root() {
    let project = Project::new();
    project.write(
        "nested/Cargo.toml",
        "[package]\nname = \"nested_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    );
    project.write("nested/src/lib.rs", "pub fn nested_call() {}\n");
    project.write(
        "nested/tests/nested_call.rs",
        "#[test]\nfn test_nested_call() { nested_fixture::nested_call(); }\n",
    );
    let base = project.commit("add nested project");
    project.write(
        "nested/src/lib.rs",
        "pub fn nested_call() { helper(); }\nfn helper() {}\n",
    );
    let head = project.commit("change nested project");

    let nested_root = project.path().join("nested");
    let output = mct(&[
        &"--root",
        &nested_root,
        &"impact",
        &"--base",
        &base,
        &"--head",
        &head,
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let report = stdout(&output);
    assert!(report.contains("src/lib.rs"), "{report}");
    assert!(
        report.contains("function nested_call (src/lib.rs:1), lines 1\n"),
        "{report}"
    );
    assert!(
        report.contains("function helper (src/lib.rs:2), lines 2\n"),
        "{report}"
    );
    assert!(report.contains("test_nested_call"), "{report}");
}
