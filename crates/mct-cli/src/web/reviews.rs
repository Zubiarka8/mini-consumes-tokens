//! The dashboard's view of review decisions, over `mct_index::reviews`.
//!
//! The shared module reads every review file (`review-decisions.toon` and
//! the older ones it never rewrites) and decides each finding's state; this
//! adapter keeps that resolution for the snapshot's findings, keyed by their
//! identity, and refreshes it after every save attempt.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use mct_index::reviews::{
    summarize, Change, FindingKeys, GroupSummary, HistoryEvent, Resolution, ReviewError,
    Reviews as Store, State,
};

pub(super) struct Reviews {
    store: Store,
    findings: Vec<FindingKeys>,
    by_key: HashMap<String, usize>,
    resolutions: Vec<Resolution>,
    histories: Vec<Vec<HistoryEvent>>,
    /// Without the group events a member shows: each event counted once.
    own_histories: Vec<Vec<HistoryEvent>>,
}

impl Reviews {
    /// `findings` are every finding of the snapshot; their identity is the
    /// key the API uses.
    pub fn load(root: &Path, findings: Vec<FindingKeys>) -> Self {
        let by_key = findings
            .iter()
            .enumerate()
            .map(|(index, finding)| (finding.identity.clone(), index))
            .collect();
        let mut reviews = Self {
            store: Store::load(root),
            findings,
            by_key,
            resolutions: Vec::new(),
            histories: Vec::new(),
            own_histories: Vec::new(),
        };
        reviews.refresh();
        reviews
    }

    fn refresh(&mut self) {
        self.resolutions = self.store.resolve(&self.findings);
        self.histories = self.store.histories(&self.findings);
        let alone: Vec<FindingKeys> = self
            .findings
            .iter()
            .cloned()
            .map(|mut finding| {
                finding.group = None;
                finding
            })
            .collect();
        self.own_histories = self.store.histories(&alone);
    }

    /// The summary of a duplicate group from its members' keys.
    pub fn summary(&self, member_keys: &[String]) -> GroupSummary {
        let members: Vec<Resolution> = member_keys
            .iter()
            .filter_map(|key| self.resolution(key).cloned())
            .collect();
        summarize(&members)
    }

    /// Every finding of the snapshot, in order.
    pub fn findings(&self) -> &[FindingKeys] {
        &self.findings
    }

    /// The identities of the findings that have a review decision (any state
    /// but `Unreviewed`). A decided member marks its group.
    pub fn reviewed_keys(&self) -> HashSet<&str> {
        self.findings
            .iter()
            .zip(&self.resolutions)
            .filter(|(_, resolution)| resolution.state() != State::Unreviewed)
            .map(|(finding, _)| {
                let owner = finding.group.and_then(|group| self.findings.get(group));
                owner.unwrap_or(finding).identity.as_str()
            })
            .collect()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.by_key.contains_key(key)
    }

    pub fn resolution(&self, key: &str) -> Option<&Resolution> {
        self.by_key.get(key).map(|index| &self.resolutions[*index])
    }

    pub fn revision(&self, key: &str) -> String {
        self.by_key.get(key).map_or_else(
            || "none".to_string(),
            |index| {
                format!(
                    "{}|{}",
                    self.findings[*index].fingerprint,
                    self.resolutions[*index].revision()
                )
            },
        )
    }

    /// Oldest first.
    pub fn history(&self, key: &str) -> &[HistoryEvent] {
        self.by_key
            .get(key)
            .map_or(&[], |index| self.histories[*index].as_slice())
    }

    /// Every finding's key with each of its own history events.
    pub fn events(&self) -> impl Iterator<Item = (&str, &HistoryEvent)> {
        self.findings
            .iter()
            .zip(&self.own_histories)
            .flat_map(|(finding, events)| {
                events
                    .iter()
                    .map(move |event| (finding.identity.as_str(), event))
            })
    }

    /// Why saving is disabled, when a review file cannot be read.
    pub fn read_only_reason(&self) -> Option<String> {
        self.store.problems().first().map(|problem| {
            let newer = if problem.newer_format {
                " (written by a newer version)"
            } else {
                ""
            };
            format!("{}{newer}: {}", problem.path.display(), problem.reason)
        })
    }

    /// Saves `state` (`Unreviewed` restores) if `revision` is still current.
    pub fn update(
        &mut self,
        key: &str,
        state: State,
        note: &str,
        revision: &str,
        now: u64,
    ) -> Result<(), ReviewError> {
        let target = *self
            .by_key
            .get(key)
            .ok_or(ReviewError::Invalid("unknown finding"))?;
        let Some((fingerprint, revision)) = revision.split_once('|') else {
            return Err(ReviewError::Conflict);
        };
        if fingerprint != self.findings[target].fingerprint {
            return Err(ReviewError::Conflict);
        }
        let change = Change {
            state,
            note: note.to_string(),
            expected_revision: revision.to_string(),
        };
        let result = self.store.save(&self.findings, target, &change, now);
        // A conflict also refreshes the store, so the page can show what
        // another writer saved.
        if matches!(result, Ok(()) | Err(ReviewError::Conflict)) {
            self.refresh();
        }
        result
    }
}
