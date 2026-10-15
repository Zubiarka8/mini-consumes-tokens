//! End-to-end tests of `mct-cli duplicates`, running the real binary against
//! throwaway projects. They assert the report's shape and the command's
//! error handling, not which functions get grouped.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A small Rust project in a fresh temp dir, removed on drop.
struct Project(PathBuf);

impl Project {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("mct-cli-duplicates-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("src/main.rs"),
            "fn alpha() {}\n\nfn main() {\n    alpha();\n}\n",
        )
        .unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn init(&self) {
        let output = mct(&[&"--root", &self.0, &"init"]);
        assert!(output.status.success(), "init failed: {}", stderr(&output));
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mct(args: &[&dyn AsRef<std::ffi::OsStr>]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mct-cli"))
        .args(args.iter().map(|arg| arg.as_ref()))
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Asserts the fixed header and the empty tail of a report with no groups.
fn assert_zero_group_report(text: &str, min_lines: &str, min_tokens: &str) {
    assert!(text.starts_with("notice: "), "{text}");
    assert!(text.contains("CANDIDATE duplicates"), "{text}");
    assert!(text.contains("human review"), "{text}");
    assert!(text.contains("does not prove"), "{text}");
    assert!(text.contains("\nstatus: complete\n"), "{text}");
    assert!(
        text.contains(&format!(
            "\noptions:\n  min_lines: {min_lines}\n  min_tokens: {min_tokens}\n"
        )),
        "{text}"
    );
    assert!(text.contains("\nsupported_languages["), "{text}");
    assert!(text.contains("\nunsupported_languages["), "{text}");
    assert!(text.contains("\ngenerated_code: "), "{text}");
    assert!(text.contains("\ncandidates_analyzed: "), "{text}");
    assert!(text.contains("\nbelow_threshold: "), "{text}");
    assert!(text.contains("\ntotal_groups: 0\n"), "{text}");
    assert!(text.contains("\ntotal_skipped: 0\n"), "{text}");
    assert!(
        text.contains(
            "\nskipped_by_reason:\n  read-error: 0\n  parse-error: 0\n  stale-location: 0\n  invalid-range: 0\n"
        ),
        "{text}"
    );
    assert!(text.contains("\ngroups[0]:\n"), "{text}");
    assert!(
        text.ends_with("\nskipped[0]{path,range,symbol,language,reason,detail}:\n"),
        "{text}"
    );
}

#[test]
fn duplicates_prints_a_valid_complete_report_with_default_thresholds() {
    let project = Project::new("defaults");
    project.init();

    let output = mct(&[&"--root", &project.path(), &"duplicates"]);
    assert!(
        output.status.success(),
        "duplicates failed: {}",
        stderr(&output)
    );
    assert_zero_group_report(&stdout(&output), "6", "40");
}

#[test]
fn custom_thresholds_are_echoed_in_the_report() {
    let project = Project::new("custom");
    project.init();

    let output = mct(&[
        &"--root",
        &project.path(),
        &"duplicates",
        &"--min-lines",
        &"12",
        &"--min-tokens",
        &"90",
    ]);
    assert!(
        output.status.success(),
        "duplicates failed: {}",
        stderr(&output)
    );
    assert_zero_group_report(&stdout(&output), "12", "90");
}

#[test]
fn the_output_is_identical_between_runs() {
    let project = Project::new("repeat");
    project.init();

    let first = mct(&[&"--root", &project.path(), &"duplicates"]);
    let second = mct(&[&"--root", &project.path(), &"duplicates"]);
    assert!(first.status.success() && second.status.success());
    assert_eq!(stdout(&first), stdout(&second));
}

#[test]
fn a_project_without_an_index_fails_clearly_and_creates_none() {
    let project = Project::new("no-index");

    let output = mct(&[&"--root", &project.path(), &"duplicates"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("no index found"), "{err}");
    assert!(err.contains("init"), "{err}");
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(
        !project.path().join(".mct-index").exists(),
        "duplicates must not create an index"
    );
}

#[test]
fn a_missing_root_fails_clearly() {
    let missing =
        std::env::temp_dir().join(format!("mct-cli-duplicates-missing-{}", std::process::id()));
    let _ = fs::remove_dir_all(&missing);

    let output = mct(&[&"--root", &missing, &"duplicates"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("is not a directory"), "{err}");
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn a_corrupt_index_fails_clearly() {
    let project = Project::new("corrupt");
    fs::create_dir_all(project.path().join(".mct-index")).unwrap();
    fs::write(
        project.path().join(".mct-index/index.sqlite3"),
        "this is not a sqlite database, just text that is long enough to be read as a header",
    )
    .unwrap();

    let output = mct(&[&"--root", &project.path(), &"duplicates"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(
        err.contains("cannot open the index") || err.contains("cannot query the index"),
        "{err}"
    );
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn invalid_thresholds_are_rejected() {
    let project = Project::new("invalid");
    project.init();

    for arg in [
        "--min-lines=0",
        "--min-lines=abc",
        "--min-lines=-4",
        "--min-tokens=0",
        "--min-tokens=1.5",
        "--min-tokens=",
    ] {
        let output = mct(&[&"--root", &project.path(), &"duplicates", &arg]);
        assert!(!output.status.success(), "`{arg}` should be rejected");
        let flag = arg.split('=').next().unwrap();
        assert!(
            stderr(&output).contains(flag),
            "error should name {flag}: {}",
            stderr(&output)
        );
        assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    }
}

#[test]
fn several_roots_are_rejected() {
    let a = Project::new("multi-a");
    let b = Project::new("multi-b");
    let output = mct(&[&"--root", &a.path(), &"--root", &b.path(), &"duplicates"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("exactly one --root"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn help_lists_the_command_and_its_thresholds() {
    let output = mct(&[&"duplicates", &"--help"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("CANDIDATE"), "{text}");
    assert!(text.contains("--min-lines"), "{text}");
    assert!(text.contains("--min-tokens"), "{text}");
    assert!(text.contains("[default: 6]"), "{text}");
    assert!(text.contains("[default: 40]"), "{text}");
    assert!(text.contains("Examples:"), "{text}");
}
