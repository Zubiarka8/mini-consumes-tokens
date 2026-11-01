//! Keeps the repository's hand-maintained ledgers honest against the files they
//! describe, so CI catches drift that otherwise only a reviewer would notice.
//!
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
