//! Human review decisions about findings (duplicate groups and dead-code
//! candidates), shared by the web dashboard, the CLI and the MCP server so a
//! decision means the same thing everywhere.
//!
//! Decisions are local preference state in `.mct-index`, never index data.
//! The only file this module writes is `review-decisions.toon` (format `C3`).
//! Older files are read where they are and never modified:
//! `duplicate-reviews.toon` (`A1` version 1 and `A2` version 2) and the
//! dashboard draft's `reviews.toon` (`B2`). Every read resolves the current
//! findings against all of them (`resolve`), so nothing is ever converted.
//!
//! A decision hides a finding only when it matches the finding's identity
//! *and* its exact code fingerprint; when any present file cannot be read,
//! nothing is hidden and nothing is written. Design and rationale:
//! `docs/superpowers/specs/2026-10-22-shared-review-decisions-design.md`.

mod format;
mod identity;
mod resolve;
mod store;

pub use identity::{dead_code_findings, duplicate_findings, FindingKeys};
pub use resolve::{Candidate, Diagnostics, HistoryEvent, Resolution};
pub use store::{Change, ReviewError, Reviews, SourceProblem, CANONICAL_FILE};

/// What a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Duplicate,
    DeadCode,
}

impl Kind {
    pub fn value(self) -> &'static str {
        match self {
            Self::Duplicate => "duplicate",
            Self::DeadCode => "dead-code",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Duplicate, Self::DeadCode]
            .into_iter()
            .find(|kind| kind.value() == value)
    }
}

/// A stored review state. `Unreviewed` is stored only as a restore tombstone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum State {
    Unreviewed,
    InReview,
    Confirmed,
    FalsePositive,
    Dismissed,
    Resolved,
}

impl State {
    pub const ALL: [State; 6] = [
        Self::Unreviewed,
        Self::InReview,
        Self::Confirmed,
        Self::FalsePositive,
        Self::Dismissed,
        Self::Resolved,
    ];

    pub fn value(self) -> &'static str {
        match self {
            Self::Unreviewed => "unreviewed",
            Self::InReview => "in-review",
            Self::Confirmed => "confirmed",
            Self::FalsePositive => "false-positive",
            Self::Dismissed => "dismissed",
            Self::Resolved => "resolved",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.value() == value)
    }

    /// Whether a decision in this state hides a finding whose fingerprint
    /// still matches.
    pub fn suppresses(self) -> bool {
        matches!(self, Self::FalsePositive | Self::Dismissed)
    }
}

/// The file format a decision was read from. The declaration order is the
/// equal-time tie-break: the newest format first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Format {
    /// `review-decisions.toon` version 3, the only format written.
    C3,
    /// `reviews.toon` version 2, from the unmerged dashboard draft.
    B2,
    /// `duplicate-reviews.toon` version 2.
    A2,
    /// `duplicate-reviews.toon` version 1 (ADR-015); has no times.
    A1,
}

impl Format {
    pub fn value(self) -> &'static str {
        match self {
            Self::C3 => "c3",
            Self::B2 => "b2",
            Self::A2 => "a2",
            Self::A1 => "a1",
        }
    }
}

/// Whether a finding belongs in an actionable list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Actionable,
    Suppressed,
}

#[cfg(test)]
mod tests;
