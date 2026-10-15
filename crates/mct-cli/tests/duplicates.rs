//! End-to-end tests of `mct-cli duplicates`, running the real binary against
//! throwaway projects: the report's shape, the command's error handling, and
//! grouping through the shipped Rust and Python token sources.

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

/// Two Rust functions with the same structure under different names and
/// literal values, in different files, plus one with a different operator.
const RUST_TWIN_A: &str = "pub fn total(items: &[u32]) -> u32 {
    // Sum the items above the limit.
    let mut sum = 0;
    for item in items {
        if *item > 10 {
            sum += item;
        }
    }
    sum
}
";
const RUST_TWIN_B: &str = "pub fn weight(values: &[u32]) -> u32 {
    let mut acc = 1;
    for value in values {
        if *value > 99 {
            acc += value;
        }
    }
    acc
}

pub fn other(values: &[u32]) -> u32 {
    let mut acc = 1;
    for value in values {
        if *value < 99 {
            acc += value;
        }
    }
    acc
}
";
const PYTHON_TWINS: &str = "def scale(rows, factor):
    out = []
    for row in rows:
        if row > 0:
            out.append(row * factor)
    return out


def shift(cells, delta):
    \"\"\"Shift every positive cell.\"\"\"
    result = []
    for cell in cells:
        if cell > 5:
            result.append(cell * delta)
    return result
";

fn grouping_report(project: &Project) -> String {
    let output = mct(&[
        &"--root",
        &project.path(),
        &"duplicates",
        &"--min-lines",
        &"4",
        &"--min-tokens",
        &"20",
    ]);
    assert!(
        output.status.success(),
        "duplicates failed: {}",
        stderr(&output)
    );
    stdout(&output)
}

#[test]
fn rust_and_python_twins_are_grouped_per_language_by_the_real_token_sources() {
    let project = Project::new("twins");
    fs::write(project.path().join("src/a.rs"), RUST_TWIN_A).unwrap();
    fs::write(project.path().join("src/b.rs"), RUST_TWIN_B).unwrap();
    fs::write(project.path().join("tools.py"), PYTHON_TWINS).unwrap();
    project.init();

    let text = grouping_report(&project);
    assert!(text.contains("\nstatus: complete\n"), "{text}");
    assert!(
        text.contains("\nsupported_languages[2]: python,rust\n"),
        "{text}"
    );
    assert!(text.contains("\ntotal_groups: 1\n"), "{text}");
    // Same structure, other names and literals; `other` differs by one
    // operator and stays out, and the comment in `total` changes nothing. In
    // Python, `shift`'s docstring is a literal statement `scale` lacks, so
    // the two do not group here (see the next test for the matching pair).
    assert!(
        text.contains("src/a.rs,1-10,total,function,rust,"),
        "{text}"
    );
    assert!(
        text.contains("src/b.rs,1-9,weight,function,rust,"),
        "{text}"
    );
    assert!(!text.contains(",other,"), "{text}");
    assert!(
        text.contains("inspect src/a.rs:1-10, src/b.rs:1-9"),
        "{text}"
    );
    assert_eq!(text.matches("\n    language: rust\n").count(), 1, "{text}");
    assert_eq!(
        text.matches("\n    language: python\n").count(),
        0,
        "{text}"
    );
}

#[test]
fn python_twins_form_one_group_and_never_join_a_rust_group() {
    let project = Project::new("python-twins");
    let python = PYTHON_TWINS.replace("    \"\"\"Shift every positive cell.\"\"\"\n", "");
    fs::write(project.path().join("tools.py"), python).unwrap();
    fs::write(project.path().join("src/a.rs"), RUST_TWIN_A).unwrap();
    project.init();

    let text = grouping_report(&project);
    assert!(text.contains("\ntotal_groups: 1\n"), "{text}");
    assert!(text.contains("\n    language: python\n"), "{text}");
    assert!(
        text.contains("tools.py,1-6,scale,function,python,"),
        "{text}"
    );
    assert!(
        text.contains("tools.py,9-14,shift,function,python,"),
        "{text}"
    );
    assert!(!text.contains("src/a.rs,"), "{text}");
}

/// Same tokens in the same order; only `total = total * row` moves out of
/// the `if`. In Python that indentation is syntax and changes behavior.
const PYTHON_INDENTATION_ONLY: &str = "def inside(flag, rows):
    total = 0
    for row in rows:
        if flag:
            total = total + row
            total = total * row
    return total


def outside(flag, rows):
    total = 0
    for row in rows:
        if flag:
            total = total + row
        total = total * row
    return total
";

#[test]
fn python_functions_differing_only_in_block_nesting_do_not_group() {
    let project = Project::new("python-indentation");
    fs::write(project.path().join("tools.py"), PYTHON_INDENTATION_ONLY).unwrap();
    project.init();

    let text = grouping_report(&project);
    assert!(text.contains("\nstatus: complete\n"), "{text}");
    assert!(text.contains("\ntotal_groups: 0\n"), "{text}");
    // Both Python functions pass the thresholds; only `src/main.rs`'s two
    // tiny Rust functions fall below them.
    assert!(text.contains("\ncandidates_analyzed: 4\n"), "{text}");
    assert!(text.contains("\nbelow_threshold: 2\n"), "{text}");
}

#[test]
fn python_functions_with_the_same_nesting_but_other_layout_still_group() {
    // A one-line suite and an indented block are the same structure.
    let project = Project::new("python-layout");
    let one_line = PYTHON_INDENTATION_ONLY
        .replace(
            "        if flag:\n            total = total + row\n        total",
            "        if flag: total = total + row\n        total",
        )
        .replace("def inside", "def unused_inside")
        .replace("def outside", "def compact");
    let source = format!("{PYTHON_INDENTATION_ONLY}\n\n{one_line}");
    fs::write(project.path().join("tools.py"), source).unwrap();
    project.init();

    let text = grouping_report(&project);
    assert!(text.contains("\ntotal_groups: 2\n"), "{text}");
    assert!(text.contains(",outside,function,python,"), "{text}");
    assert!(text.contains(",compact,function,python,"), "{text}");
    assert!(
        text.contains("inspect tools.py:10-16, tools.py:28-33"),
        "{text}"
    );
}

#[test]
fn the_first_and_last_methods_of_a_class_group_like_any_other() {
    // The class body starts on `first`'s first line and ends on `last`'s last
    // line; that enclosing block must not leak into either method.
    let method = |name: &str| {
        format!(
            "    def {name}(self, rows):
        total = 0
        for row in rows:
            if row:
                total = total + row
        return total
"
        )
    };
    let source = format!("class Box:\n{}\n{}", method("first"), method("last"));
    let project = Project::new("python-class-edges");
    fs::write(project.path().join("tools.py"), source).unwrap();
    project.init();

    let text = grouping_report(&project);
    assert!(text.contains("\ntotal_groups: 1\n"), "{text}");
    assert!(
        text.contains("inspect tools.py:2-7, tools.py:9-14"),
        "{text}"
    );
}

#[test]
fn a_file_edited_after_indexing_is_skipped_as_stale_not_grouped() {
    let project = Project::new("stale");
    fs::write(project.path().join("src/a.rs"), RUST_TWIN_A).unwrap();
    fs::write(project.path().join("src/b.rs"), RUST_TWIN_B).unwrap();
    project.init();
    fs::write(
        project.path().join("src/b.rs"),
        format!("// edited\n{RUST_TWIN_B}"),
    )
    .unwrap();

    let text = grouping_report(&project);
    assert!(text.contains("\nstatus: partial\n"), "{text}");
    assert!(text.contains("\ntotal_groups: 0\n"), "{text}");
    assert!(text.contains("\n  stale-location: 2\n"), "{text}");
    assert!(
        text.contains("src/b.rs,1-9,weight,rust,stale-location,"),
        "{text}"
    );
}
