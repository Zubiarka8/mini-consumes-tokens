//! The early-regression gate (issue #21): `cargo test --workspace` runs the
//! quality suite on every PR and fails on any regression against
//! `baseline.json`. Run with `--nocapture` to see the full report.
//!
//! After an intended change (a new case, a better ranking, a deliberate
//! output change), refresh the baseline with
//! `cargo run -p mct-eval -- --write-baseline` and commit it.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_eval::{compare, render_markdown, Baseline, Suite, Thresholds};

#[tokio::test]
async fn the_quality_suite_does_not_regress_against_the_baseline() {
    let suite = Suite::load(&mct_eval::default_suite_path()).unwrap();
    let baseline = Baseline::load(&mct_eval::default_baseline_path()).unwrap();
    let report = mct_eval::run(&suite, 3).await.unwrap();
    let comparison = compare(&report, &baseline, &Thresholds::default());
    println!("{}", render_markdown(&report, Some(&comparison)));
    assert!(
        comparison.regressions.is_empty(),
        "quality regressed against crates/mct-eval/baseline.json:\n  {}\n\
         If the change is intended, refresh it: cargo run -p mct-eval -- --write-baseline",
        comparison.regressions.join("\n  ")
    );
}

#[tokio::test]
async fn every_case_is_well_formed() {
    let suite = Suite::load(&mct_eval::default_suite_path()).unwrap();
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
