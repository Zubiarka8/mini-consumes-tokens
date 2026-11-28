//! `mct-cli impact --base` (issue #632): the changed lines since the merge
//! base, mapped to the definitions they fall in, each with its callers and
//! the tests that reach it; removed definitions still named; stale files
//! reported as partial.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use git2::{IndexAddOption, Repository, Signature};

#[path = "common/command.rs"]
mod command;
use command::mct;

const LIB_RS: &str = "pub fn parse_item() {
    validate();
}
fn validate() {}
pub fn retired() {}
pub fn keep() {
    retired();
}
";

struct Project(tempfile::TempDir);

impl Project {
    fn new() -> Self {
        let project = Self(tempfile::tempdir().unwrap());
        project.write(
            "Cargo.toml",
            "[package]\nname = \"impact_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        );
        project.write("src/lib.rs", LIB_RS);
        // A homonym of `validate` that the change does not touch.
        project.write("src/other.rs", "pub fn validate() {}\n");
        project.write(
            "tests/parse_item.rs",
            "#[test]\nfn test_parse_item() {\n    impact_fixture::parse_item();\n}\n",
        );
        project.commit("base");
        project
    }

    fn path(&self) -> &Path {
        self.0.path()
    }

    fn write(&self, path: &str, contents: &str) {
        let path = self.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn commit(&self, message: &str) -> String {
        let repo = Repository::open(self.path())
            .unwrap_or_else(|_| Repository::init(self.path()).unwrap());
        let mut index = repo.index().unwrap();
        index
            .add_all(
                ["Cargo.toml", "src", "tests", "README.md"],
                IndexAddOption::DEFAULT,
                None,
            )
            .unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = Signature::now("Test", "test@example.com").unwrap();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<_> = parent.iter().collect();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )
        .unwrap()
        .to_string()
    }

    fn impact(&self, args: &[&str]) -> String {
        let root = self.path();
        let mut argv: Vec<&dyn AsRef<std::ffi::OsStr>> = vec![&"--root", &root, &"impact"];
        for arg in args {
            argv.push(arg);
        }
        let output = mct(&argv);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

#[test]
fn a_changed_function_names_its_callers_and_tests_and_not_its_homonym() {
    let project = Project::new();
    let base = project.commit("noop");
    // `validate`'s body changes, uncommitted: the working tree is the head.
    project.write(
        "src/lib.rs",
        &LIB_RS.replace("fn validate() {}", "fn validate() { let _ = 1; }"),
    );
    let report = project.impact(&["--base", &base]);
    assert!(
        report.contains("1 file(s) changed, 1 definition(s) changed, 0 removed.\n"),
        "{report}"
    );
    assert!(
        report.contains("function validate (src/lib.rs:4), lines 4\n"),
        "{report}"
    );
    assert!(
        report.contains("  caller parse_item (src/lib.rs:2)\n"),
        "{report}"
    );
    assert!(
        report.contains("  test test_parse_item (tests/parse_item.rs:3) — #[test], confirmed\n"),
        "{report}"
    );
    assert!(!report.contains("src/other.rs"), "{report}");
}

#[test]
fn a_deleted_function_is_named_with_what_still_calls_it() {
    let project = Project::new();
    project.write("src/lib.rs", &LIB_RS.replace("pub fn retired() {}\n", ""));
    let head = project.commit("drop retired");
    let report = project.impact(&["--base", "HEAD~1", "--head", &head]);
    assert!(
        report.contains(
            "removed function retired (src/lib.rs at base line 5), still named by 1 relation(s)\n  \
             keep (src/lib.rs:6)\n"
        ),
        "{report}"
    );
    // Deleting a function is no change to its neighbors.
    assert!(
        report.contains(", 0 definition(s) changed, 1 removed.\n"),
        "{report}"
    );
}

#[test]
fn a_head_the_index_does_not_hold_is_reported_partial() {
    let project = Project::new();
    let base = project.commit("noop");
    project.write(
        "src/lib.rs",
        &LIB_RS.replace("fn validate() {}", "fn validate() { let _ = 1; }"),
    );
    let head = project.commit("change validate");
    // The working tree moves on; the index follows it, not `head`.
    project.write("src/lib.rs", &format!("\n\n{LIB_RS}"));
    let report = project.impact(&["--base", &base, "--head", &head]);
    assert!(
        report.contains("Partial: 1 file(s) indexed from other bytes than the head's"),
        "{report}"
    );
    assert!(report.contains("src/lib.rs\n"), "{report}");
}

#[test]
fn a_change_outside_any_definition_says_so() {
    let project = Project::new();
    let base = project.commit("noop");
    let none = project.impact(&["--base", &base, "--head", &base]);
    assert!(
        none.contains("No file changed since the merge base.\n"),
        "{none}"
    );
    // A comment after the last function: only the file's module holds it.
    project.write("src/lib.rs", &format!("{LIB_RS}// trailing note\n"));
    let head = project.commit("comment");
    let report = project.impact(&["--base", &base, "--head", &head]);
    assert!(
        report.contains("0 definition(s) changed, 0 removed.\n"),
        "{report}"
    );
    assert!(
        report.contains("No indexed definition covers the change in: src/lib.rs\n"),
        "{report}"
    );
}
