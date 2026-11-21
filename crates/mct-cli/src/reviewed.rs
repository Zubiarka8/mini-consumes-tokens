//! `--reviewed`: applies the human review decisions of `mct_index::reviews`
//! to a CLI report. Without the flag no review file is read and every report
//! is exactly what it was before the flag existed.

use std::path::Path;

use mct_index::reviews::{Resolution, Reviews, State, Visibility};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Reviewed {
    /// Every finding, with its review state.
    Show,
    /// Only findings that still need a person: false positives and dismissed
    /// findings whose code is unchanged are left out.
    Hide,
    /// Only the findings `hide` leaves out.
    Only,
}

impl Reviewed {
    pub(crate) fn keeps(self, resolution: &Resolution) -> bool {
        self.keeps_visibility(resolution.visibility())
    }

    pub(crate) fn keeps_visibility(self, visibility: Visibility) -> bool {
        match self {
            Self::Show => true,
            Self::Hide => visibility == Visibility::Actionable,
            Self::Only => visibility == Visibility::Suppressed,
        }
    }
}

/// Reads the project's review files. If one cannot be read, says so on
/// stderr: nothing is hidden then, and the report is still produced.
pub(crate) fn load(root: &Path) -> Reviews {
    let reviews = Reviews::load(root);
    for problem in reviews.problems() {
        eprintln!(
            "warning: review file {} cannot be read ({}); no finding is hidden",
            problem.path.display(),
            problem.reason
        );
    }
    reviews
}

/// `"k hidden by review (false-positive a, dismissed b)"`, or `None` when
/// nothing is hidden.
pub(crate) fn hidden_summary(resolutions: &[Resolution]) -> Option<String> {
    hidden_summary_of(
        resolutions
            .iter()
            .map(|r| (r.visibility(), r.state().value())),
    )
}

/// [`hidden_summary`] over `(visibility, label)` pairs, such as the
/// [`mct_index::reviews::GroupSummary`] of duplicate groups.
pub(crate) fn hidden_summary_of<'a>(
    items: impl Iterator<Item = (Visibility, &'a str)> + Clone,
) -> Option<String> {
    let count = |state: State| {
        items
            .clone()
            .filter(|(visibility, label)| {
                *visibility == Visibility::Suppressed && *label == state.value()
            })
            .count()
    };
    let (false_positive, dismissed) = (count(State::FalsePositive), count(State::Dismissed));
    let hidden = false_positive + dismissed;
    (hidden > 0).then(|| {
        format!(
            "{hidden} hidden by review (false-positive {false_positive}, dismissed {dismissed})"
        )
    })
}
