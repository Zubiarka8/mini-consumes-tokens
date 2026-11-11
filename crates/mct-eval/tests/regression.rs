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

#[test]
fn a_bundled_fixture_is_indexed_outside_the_repository_and_removed_after() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let suite = Suite::load(&dir.join("suite-resolution.json")).unwrap();
    // Checked in as one bundle, not as source files MCT would index.
    assert!(dir.join(&suite.fixture).is_file(), "{}", suite.fixture);
    assert!(!dir.join("tests/fixtures/resolution-app").exists());

    let root = suite.fixture_root.clone();
    assert!(root.join("rust/src/lib.rs").is_file());
    let repo = dir.join("../..").canonicalize().unwrap();
    assert!(
        !root.canonicalize().unwrap().starts_with(&repo),
        "{} is inside the repository",
        root.display()
    );
    drop(suite);
    assert!(!root.exists(), "{} was left behind", root.display());
}

#[test]
fn no_parser_reads_a_fixture_bundle() {
    let registry = mct_mcp_server::registry::build_registry();
    for suite_path in suite_paths() {
        let suite = Suite::load(&suite_path).unwrap();
        let fixture = suite_path.with_file_name(&suite.fixture);
        if fixture.is_file() {
            let extension = fixture.extension().and_then(|e| e.to_str()).unwrap();
            assert!(
                registry.for_extension(extension).is_none(),
                "{} would be indexed with the repository",
                fixture.display()
            );
        }
    }
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
            !case.expect.is_empty() || !case.absent.is_empty() || !case.labels.is_empty(),
            "`{}` checks nothing",
            case.name
        );
        for label in &case.labels {
            assert_label_well_formed(suite, &case.name, label);
        }
        assert!(
            !case.ranked || !case.expect.is_empty(),
            "`{}` is ranked with nothing to rank",
            case.name
        );
    }
}

/// A label names a site as `path:line[:col]`, a relation as `<kind> <name>`
/// and a truth that is `external` or a real `path:line` of the fixture.
fn assert_label_well_formed(suite: &Suite, case: &str, label: &mct_eval::resolution::Label) {
    let parts: Vec<&str> = label.at.split(':').collect();
    assert!(
        (2..=3).contains(&parts.len()) && parts[1..].iter().all(|p| p.parse::<u32>().is_ok()),
        "`{case}`: bad site `{}`",
        label.at
    );
    assert!(
        suite.fixture_root.join(parts[0]).is_file(),
        "`{case}`: no fixture file `{}`",
        parts[0]
    );
    assert!(
        label.relation.split_once(' ').is_some_and(|(kind, name)| {
            [
                "calls",
                "references",
                "imports",
                "extends",
                "implements",
                "overrides",
            ]
            .contains(&kind)
                && !name.is_empty()
        }),
        "`{case}`: bad relation `{}`",
        label.relation
    );
    if label.truth != "external" {
        let (path, line) = label.truth.split_once(':').unwrap_or_default();
        let line: usize = line.parse().unwrap_or(0);
        let source = std::fs::read_to_string(suite.fixture_root.join(path)).unwrap_or_default();
        assert!(
            line >= 1 && line <= source.lines().count(),
            "`{case}`: truth `{}` is not a fixture line",
            label.truth
        );
    }
}
