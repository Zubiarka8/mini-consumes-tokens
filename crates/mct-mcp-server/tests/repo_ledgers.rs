//! Keeps the repository's hand-maintained ledgers honest against the files they
//! describe, so CI catches drift that otherwise only a reviewer would notice.
//!
//! - Every `#[ignore]`d test must have an entry in `ISSUES_PENDING.md`.
//! - `internal/corpus-progress.md` must be internally consistent: valid statuses,
//!   a PR number on every started row, and a summary line that matches the rows.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_repo_file(relative: &str) -> String {
    std::fs::read_to_string(repo_root().join(relative)).unwrap()
}

/// Every `.rs` file under `dir`, recursively.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The name of the function declared on `line`, if the line declares one.
fn fn_name(line: &str) -> Option<String> {
    let mut words = line.split_whitespace();
    words.by_ref().find(|word| *word == "fn")?;
    let name: String = words
        .next()?
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// `(file, name)` of every ignored test under `crates/` that is a pending bug.
/// A test whose reason starts with `not a bug:` is parked on purpose (a report,
/// a benchmark) and needs no ledger entry; every other `#[ignore]` pins a bug.
fn ignored_tests() -> Vec<(String, String)> {
    let mut files = Vec::new();
    rust_files(&repo_root().join("crates"), &mut files);
    let mut found = Vec::new();
    for file in files {
        let Ok(source) = std::fs::read_to_string(&file) else {
            continue;
        };
        let relative = file
            .strip_prefix(repo_root())
            .unwrap_or(&file)
            .display()
            .to_string();
        // `#[ignore]` applies to the next function; other attributes may sit between.
        let mut pending = false;
        for line in source.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("#[ignore") && !trimmed.starts_with("#[ignore = \"not a bug:") {
                pending = true;
            } else if pending {
                if let Some(name) = fn_name(trimmed) {
                    found.push((relative.clone(), name));
                    pending = false;
                }
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

#[test]
fn every_ignored_test_is_recorded_in_issues_pending() {
    let ledger = read_repo_file("ISSUES_PENDING.md");
    // Names are backticked in the ledger, so `name` must not match inside another name.
    let undocumented: Vec<String> = ignored_tests()
        .into_iter()
        .filter(|(_, name)| !ledger.contains(&format!("`{name}`")))
        .map(|(file, name)| format!("{file}: {name}"))
        .collect();
    assert!(
        undocumented.is_empty(),
        "these #[ignore]d tests have no entry in ISSUES_PENDING.md (add one, \
         naming the test in backticks): {undocumented:#?}"
    );
}

/// The number written immediately before `label` in `text`, e.g. `7` in `7 done`.
fn count_before(text: &str, label: &str) -> usize {
    let head = &text[..text
        .find(label)
        .unwrap_or_else(|| panic!("no `{label}` in {text}"))];
    head.split_whitespace()
        .last()
        .unwrap()
        .trim_start_matches('(')
        .parse()
        .unwrap()
}

#[test]
fn corpus_progress_rows_and_summary_agree() {
    let progress = read_repo_file("internal/corpus-progress.md");
    let mut done = 0;
    let mut in_pr = 0;
    let mut pending = 0;
    for line in progress.lines() {
        if !line.starts_with('|') || !line.contains("`mct-lang-") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let (language, status, pr) = (cells[1], cells[3].trim_matches('*'), cells[4]);
        let has_pr = pr.starts_with('#');
        match status {
            "Done" => {
                done += 1;
                assert!(has_pr, "{language}: status Done needs its PR number");
            }
            "In PR" => {
                in_pr += 1;
                assert!(has_pr, "{language}: status In PR needs its PR number");
            }
            "Pending" => {
                pending += 1;
                assert!(!has_pr, "{language}: status Pending must not name a PR");
            }
            other => panic!("{language}: unknown status `{other}`"),
        }
    }
    let summary = progress
        .lines()
        .find(|line| line.starts_with("**Summary:"))
        .expect("corpus-progress.md has no `**Summary:` line");
    assert_eq!(count_before(summary, "done"), done, "Done rows vs summary");
    assert_eq!(
        count_before(summary, "in PR"),
        in_pr,
        "In PR rows vs summary"
    );
    assert_eq!(
        count_before(summary, "pending"),
        pending,
        "Pending rows vs summary"
    );
    assert_eq!(
        count_before(summary, "total"),
        done + in_pr + pending,
        "total vs summary"
    );
}
