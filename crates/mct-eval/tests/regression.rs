//! The early-regression gate (issue #21): `cargo test --workspace` runs
//! every quality suite ([`mct_eval::SUITES`]) on every PR and fails on any
//! regression against its baseline. Run with `--nocapture` to see the full
//! reports.
//!
//! After an intended change (a new case, a better ranking, a deliberate
//! output change), refresh the baseline with
//! `cargo run -p mct-eval -- [--suite crates/mct-eval/<suite>.json] --write-baseline`
//! and commit it.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use mct_eval::{baseline_path_for, compare, render_markdown, Baseline, Suite, Thresholds};

fn suite_paths() -> Vec<PathBuf> {
    mct_eval::SUITES
        .iter()
        .map(|name| Path::new(env!("CARGO_MANIFEST_DIR")).join(name))
        .collect()
}

#[tokio::test]
async fn no_quality_suite_regresses_against_its_baseline() {
    for suite_path in suite_paths() {
        let baseline_path = baseline_path_for(&suite_path);
        let suite = Suite::load(&suite_path).unwrap();
        let baseline = Baseline::load(&baseline_path).unwrap();
        let report = mct_eval::run(&suite, 3).await.unwrap();
        let comparison = compare(&report, &baseline, &Thresholds::default());
        println!("{}", render_markdown(&report, Some(&comparison)));
        assert!(
            comparison.regressions.is_empty(),
            "quality regressed against {}:\n  {}\n\
             If the change is intended, refresh it: \
             cargo run -p mct-eval -- --suite {} --write-baseline",
            baseline_path.display(),
            comparison.regressions.join("\n  "),
            suite_path.display()
        );
    }
}

#[test]
fn each_suite_has_its_own_baseline() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(
        baseline_path_for(&dir.join("suite.json")),
        mct_eval::default_baseline_path()
    );
    assert_eq!(
        baseline_path_for(&dir.join("suite-portfolio-3d.json")),
        dir.join("baseline-portfolio-3d.json")
    );
}

#[tokio::test]
async fn every_case_is_well_formed() {
    for suite_path in suite_paths() {
        assert_cases_well_formed(&Suite::load(&suite_path).unwrap());
    }
}

fn assert_cases_well_formed(suite: &Suite) {
    let mut names = std::collections::HashSet::new();
    for case in &suite.cases {
        assert!(
            names.insert(case.name.as_str()),
            "duplicate case name `{}`",
            case.name
        );
        assert!(
            !case.expect.is_empty() || !case.absent.is_empty(),
            "`{}` checks nothing",
            case.name
        );
        assert!(
            !case.ranked || !case.expect.is_empty(),
            "`{}` is ranked with nothing to rank",
            case.name
        );
    }
}
