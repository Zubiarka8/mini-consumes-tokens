//! The CLI reports that have an MCP twin give the same findings: `dead-code`
//! and `find_dead_code`, `duplicates` and `find_duplicate_code`. Each is
//! read from the same index, so a difference is a bug in one front end.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::fs;
use tempfile::TempDir;

#[path = "common/command.rs"]
mod command;
use command::mct;

/// A project with unused functions in two directories and one duplicated
/// function pair, in a fresh temp dir removed on drop.
struct Project(TempDir);

impl Project {
    fn new(tag: &str) -> Self {
        let temp = tempfile::Builder::new()
            .prefix(&format!("mct-cli-parity-{tag}-"))
            .tempdir()
            .unwrap();
        let dir = temp.path();
        fs::create_dir_all(dir.join("src/util")).unwrap();
        let twin = |name: &str| {
            format!(
                "pub fn {name}(items: &[u32]) -> u32 {{\n    let mut total = 0;\n    for item in items {{\n        if *item > 10 {{\n            total += item * 2;\n        }} else {{\n            total += item;\n        }}\n    }}\n    total\n}}\n"
            )
        };
        fs::write(
            dir.join("src/main.rs"),
            format!(
                "fn used() {{}}\n\nfn unused_root() {{}}\n\nfn main() {{\n    used();\n}}\n\n{}",
                twin("sum_a")
            ),
        )
        .unwrap();
        fs::write(
            dir.join("src/util/mod.rs"),
            format!("pub fn unused_util() {{}}\n\n{}", twin("sum_b")),
        )
        .unwrap();
        let init = mct(&[&"--root", &dir, &"init"]);
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        Self(temp)
    }
}

fn stdout(args: &[&dyn AsRef<std::ffi::OsStr>]) -> String {
    let output = mct(args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// `(path, name, line)` of each row of the CLI's dead-code CSV.
fn cli_dead_code(project: &Project, path: Option<&str>) -> BTreeSet<(String, String, String)> {
    let csv = project.0.path().join("dead.csv");
    let root = project.0.path();
    let mut args: Vec<&dyn AsRef<std::ffi::OsStr>> =
        vec![&"--root", &root, &"dead-code", &"--output", &csv];
    if let Some(path) = &path {
        args.push(&"--path");
        args.push(path);
    }
    stdout(&args);
    let text = fs::read_to_string(&csv).unwrap();
    text.lines()
        .skip(1)
        .map(|row| {
            let cells: Vec<&str> = row.split(',').collect();
            (
                cells[1].to_string(),
                cells[2].to_string(),
                cells[5].to_string(),
            )
        })
        .collect()
}

/// `(path, name, line)` of each `find_dead_code` candidate row.
fn mcp_dead_code(project: &Project, path: Option<&str>) -> BTreeSet<(String, String, String)> {
    let args = match path {
        Some(path) => format!(r#"{{"limit":1000,"path":"{path}"}}"#),
        None => r#"{"limit":1000}"#.to_string(),
    };
    let text = stdout(&[
        &"--root",
        &project.0.path(),
        &"call",
        &"--tool",
        &"find_dead_code",
        &"--args",
        &args,
    ]);
    text.lines()
        .skip_while(|l| !l.starts_with("candidates["))
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .map(|row| {
            let cells: Vec<&str> = row.trim().split(',').collect();
            (
                cells[2].to_string(),
                cells[1].to_string(),
                cells[3].to_string(),
            )
        })
        .collect()
}

#[test]
fn dead_code_lists_the_same_candidates_from_the_cli_and_the_tool() {
    let project = Project::new("dead");
    let whole = cli_dead_code(&project, None);
    assert!(
        whole.iter().any(|(_, name, _)| name == "unused_util"),
        "{whole:?}"
    );
    assert_eq!(whole, mcp_dead_code(&project, None));
    for path in ["src/util", "src/util/", "./src/util"] {
        let scoped = cli_dead_code(&project, Some(path));
        assert_eq!(scoped, mcp_dead_code(&project, Some(path)), "{path}");
        assert!(
            scoped
                .iter()
                .all(|(file, _, _)| file.starts_with("src/util/"))
                && !scoped.is_empty(),
            "{path}: {scoped:?}"
        );
    }
}

/// The value of the first `key: value` line.
fn field<'t>(text: &'t str, key: &str) -> &'t str {
    text.lines()
        .find_map(|l| l.strip_prefix(&format!("{key}: ")))
        .unwrap_or_else(|| panic!("no `{key}` in\n{text}"))
}

#[test]
fn duplicates_report_the_same_members_and_counts_from_the_cli_and_the_tool() {
    let project = Project::new("dup");
    let cli = stdout(&[&"--root", &project.0.path(), &"duplicates"]);
    let mcp = stdout(&[
        &"--root",
        &project.0.path(),
        &"call",
        &"--tool",
        &"find_duplicate_code",
        &"--args",
        &"{}",
    ]);
    assert_eq!(field(&cli, "total_groups"), "1", "{cli}");
    assert_eq!(field(&cli, "total_groups"), field(&mcp, "detected"));
    for key in ["candidates_analyzed", "below_threshold"] {
        assert_eq!(field(&cli, key), field(&mcp, key), "{key}");
    }
    assert_eq!(field(&mcp, "members_omitted"), "0");
    assert_eq!(field(&mcp, "truncated"), "false");
    let cli_members: BTreeSet<_> = cli
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("members[2]{"))
        .skip(1)
        .take(2)
        .map(|line| {
            let cells: Vec<_> = line.trim().split(',').collect();
            (
                cells[0].to_string(),
                cells[2].to_string(),
                cells[1].to_string(),
            )
        })
        .collect();
    let mcp_members: BTreeSet<_> = mcp
        .lines()
        .skip_while(|line| !line.starts_with("members[2]{"))
        .skip(1)
        .take(2)
        .map(|line| {
            let cells: Vec<_> = line.trim().split(',').collect();
            (
                cells[1].to_string(),
                cells[2].to_string(),
                format!("{}-{}", cells[4], cells[6]),
            )
        })
        .collect();
    assert_eq!(cli_members.len(), 2, "{cli}");
    assert_eq!(cli_members, mcp_members);
    assert_eq!(
        cli_members
            .iter()
            .map(|(_, name, _)| name.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["sum_a", "sum_b"])
    );
}

#[test]
fn status_reports_what_get_indexing_status_reports() {
    let project = Project::new("status");
    let cli = stdout(&[&"--root", &project.0.path(), &"status"]);
    let mcp = stdout(&[
        &"--root",
        &project.0.path(),
        &"call",
        &"--tool",
        &"get_indexing_status",
        &"--args",
        &"{}",
    ]);
    // The tool also says when its own process last checked the files, and
    // `call` ends its output with a blank line.
    let without_process_line = |text: &str| -> Vec<String> {
        text.trim_end()
            .lines()
            .filter(|l| !l.starts_with("Last verified unchanged"))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(without_process_line(&cli), without_process_line(&mcp));
}
