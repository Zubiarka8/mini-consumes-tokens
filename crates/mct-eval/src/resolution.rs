//! Labelled resolution cases: what one relation in the tool output resolved
//! to, checked against a hand-verified truth.
//!
//! A [`Label`] names one relation site (`path:line[:col]`, its kind and
//! target name) and what the source really refers to there: a definition in
//! the fixture (`path:line`) or something outside it (`external`). The truth
//! is checked by compiling or running the fixture, never read off the tool.
//! Each label then gets a [`Verdict`]: correct, **wrong** (a resolved edge to
//! the wrong place, or `external` for a name the fixture defines), or
//! neither (ambiguous, unresolved, missing), so a run reports wrong edges
//! separately from the share it resolved.

use serde::{Deserialize, Serialize};

/// One relation site and the definition it really refers to.
#[derive(Debug, Clone, Deserialize)]
pub struct Label {
    /// Where the relation is: `path:line` or `path:line:col`.
    pub at: String,
    /// `<kind> <target name>`, as the output writes them: `calls Ok`.
    pub relation: String,
    /// `external`, or the definition's `path:line`.
    pub truth: String,
    /// The audit defect a wrong answer here reproduces (`D3`), if any.
    #[serde(default)]
    pub defect: Option<String>,
}

/// What the output says the relation at a label's site resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed {
    /// `-> path:line …` (or `-> Lline …` for a target in the same file),
    /// with the target normalized to `path:line`.
    Resolved(String),
    Ambiguous,
    Unresolved,
    External,
    /// No line of the output is this relation.
    Missing,
    /// More than one line matches: the label needs a column.
    Duplicate,
}

/// How a label's observed resolution compares with its truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Correct,
    /// A resolved edge to anything but the truth, or `external` for a name
    /// the fixture defines.
    Wrong,
    Ambiguous,
    Unresolved,
    /// The relation isn't in the output (or the label matched twice).
    Missing,
}

/// One label's outcome, kept in the JSON report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelResult {
    pub at: String,
    pub relation: String,
    pub truth: String,
    pub observed: String,
    pub verdict: Verdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defect: Option<String>,
}

/// The relation at `label`'s site in a relation tool's text output.
pub fn observe(text: &str, label: &Label) -> Observed {
    let Some((kind, name)) = label.relation.split_once(' ') else {
        return Observed::Missing;
    };
    let arrow = format!(" --{kind}--> {name}");
    // `path:line` matches every column on that line; `path:line:col` one.
    let with_col = label.at.matches(':').count() >= 2;
    let mut found = text.lines().filter_map(|line| {
        let rest = line.strip_prefix(label.at.as_str())?;
        let at_site = if with_col {
            rest.starts_with(' ')
        } else {
            rest.starts_with(':')
        };
        let tail = rest.split_once(arrow.as_str())?.1;
        // The name ends here: `Ok` is not `Okay`.
        (at_site && (tail.is_empty() || tail.starts_with(' '))).then_some(tail)
    });
    let Some(tail) = found.next() else {
        return Observed::Missing;
    };
    if found.next().is_some() {
        return Observed::Duplicate;
    }
    if let Some(target) = tail.strip_prefix(" -> ") {
        let target = target.split(" in ").next().unwrap_or(target);
        let target = target.split(" [depth").next().unwrap_or(target);
        let target = target.split(" (by name)").next().unwrap_or(target).trim();
        let path = label.at.split(':').next().unwrap_or_default();
        return Observed::Resolved(match target.strip_prefix('L') {
            Some(line) if line.chars().all(|c| c.is_ascii_digit()) => format!("{path}:{line}"),
            _ => target.to_string(),
        });
    }
    if tail.starts_with(" (external)") {
        Observed::External
    } else if tail.starts_with(" (unresolved)") {
        Observed::Unresolved
    } else if tail.starts_with(" (ambiguous") {
        Observed::Ambiguous
    } else {
        // Resolved, but the output named no candidate.
        Observed::Resolved(String::new())
    }
}

/// `observed` judged against `truth` (`external` or `path:line`).
pub fn verdict(truth: &str, observed: &Observed) -> Verdict {
    match observed {
        Observed::Resolved(target) if target == truth => Verdict::Correct,
        Observed::Resolved(_) => Verdict::Wrong,
        Observed::External if truth == "external" => Verdict::Correct,
        Observed::External => Verdict::Wrong,
        Observed::Ambiguous => Verdict::Ambiguous,
        Observed::Unresolved => Verdict::Unresolved,
        Observed::Missing | Observed::Duplicate => Verdict::Missing,
    }
}

/// Every label of a case, observed and judged.
pub fn judge(text: &str, labels: &[Label]) -> Vec<LabelResult> {
    labels
        .iter()
        .map(|label| {
            let observed = observe(text, label);
            LabelResult {
                at: label.at.clone(),
                relation: label.relation.clone(),
                truth: label.truth.clone(),
                observed: describe(&observed),
                verdict: verdict(&label.truth, &observed),
                defect: label.defect.clone(),
            }
        })
        .collect()
}

fn describe(observed: &Observed) -> String {
    match observed {
        Observed::Resolved(target) => target.clone(),
        Observed::Ambiguous => "ambiguous".into(),
        Observed::Unresolved => "unresolved".into(),
        Observed::External => "external".into(),
        Observed::Missing => "missing".into(),
        Observed::Duplicate => "duplicate".into(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    const OUTPUT: &str = "\
5 reference(s):
rust/src/imports.rs:4:5 [rust] accept --calls--> Ok -> rust/src/kinds.rs:5 in Outcome
rust/src/shadow.rs:4:5 [rust] wrap --calls--> Some -> L1 (by name)
rust/src/prelude_use.rs:11:15 [rust] parse --calls--> Ok (external)
rust/src/same_name.rs:17:6 [rust] sizes --calls--> len (ambiguous among 2: L7 in Stack, L12)
rust/src/glob.rs:4:5 [rust] reject --calls--> Err (unresolved)
rust/src/twice.rs:3:9 [rust] f --references--> String (external)
rust/src/twice.rs:3:17 [rust] f --references--> String -> rust/src/kinds.rs:9 in Kind
rust/src/prelude_use.rs:11:15 [rust] parse --calls--> Okay (external)
";

    fn label(at: &str, relation: &str) -> Label {
        Label {
            at: at.into(),
            relation: relation.into(),
            truth: "external".into(),
            defect: None,
        }
    }

    #[test]
    fn observes_each_resolution_form() {
        let cases = [
            (
                "rust/src/imports.rs:4",
                "calls Ok",
                Observed::Resolved("rust/src/kinds.rs:5".into()),
            ),
            // A same-file target is printed as `Lline`; a target left by
            // a unique name alone is marked `(by name)` after it.
            (
                "rust/src/shadow.rs:4",
                "calls Some",
                Observed::Resolved("rust/src/shadow.rs:1".into()),
            ),
            ("rust/src/prelude_use.rs:11", "calls Ok", Observed::External),
            ("rust/src/same_name.rs:17", "calls len", Observed::Ambiguous),
            ("rust/src/glob.rs:4", "calls Err", Observed::Unresolved),
            ("rust/src/glob.rs:4", "calls Ok", Observed::Missing),
            // A kind mismatch is another relation.
            ("rust/src/imports.rs:4", "references Ok", Observed::Missing),
            (
                "rust/src/twice.rs:3",
                "references String",
                Observed::Duplicate,
            ),
            (
                "rust/src/twice.rs:3:17",
                "references String",
                Observed::Resolved("rust/src/kinds.rs:9".into()),
            ),
        ];
        for (at, relation, want) in cases {
            assert_eq!(
                observe(OUTPUT, &label(at, relation)),
                want,
                "{at} {relation}"
            );
        }
    }

    #[test]
    fn a_line_prefix_is_not_another_line() {
        // `…:1` must not match `…:11`, nor `Ok` the name `Okay`.
        assert_eq!(
            observe(OUTPUT, &label("rust/src/prelude_use.rs:1", "calls Ok")),
            Observed::Missing
        );
    }

    #[test]
    fn verdicts_separate_wrong_edges_from_undecided_ones() {
        let target = "rust/src/kinds.rs:5";
        let at = |s: &str| Observed::Resolved(s.into());
        for (truth, observed, want) in [
            (target, at(target), Verdict::Correct),
            (target, at("rust/src/other.rs:5"), Verdict::Wrong),
            // `external` for a name the fixture defines is a wrong answer.
            (target, Observed::External, Verdict::Wrong),
            (target, Observed::Ambiguous, Verdict::Ambiguous),
            (target, Observed::Unresolved, Verdict::Unresolved),
            ("external", Observed::External, Verdict::Correct),
            ("external", at(target), Verdict::Wrong),
            ("external", Observed::Ambiguous, Verdict::Ambiguous),
            ("external", Observed::Unresolved, Verdict::Unresolved),
            ("external", Observed::Missing, Verdict::Missing),
            ("external", Observed::Duplicate, Verdict::Missing),
        ] {
            assert_eq!(verdict(truth, &observed), want, "{truth} vs {observed:?}");
        }
    }
}
