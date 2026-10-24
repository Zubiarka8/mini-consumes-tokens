//! The dashboard's view of review decisions, over `mct_index::reviews`.
//!
//! The shared module reads every review file (`review-decisions.toon` and
//! the older ones it never rewrites) and decides each finding's state; this
//! adapter keeps that resolution for the snapshot's findings, keyed by their
//! identity, and refreshes it after every save attempt.

use std::collections::HashMap;
use std::path::Path;

use mct_index::reviews::{
    Change, FindingKeys, HistoryEvent, Resolution, ReviewError, Reviews as Store, State,
};

pub(super) struct Reviews {
    store: Store,
    findings: Vec<FindingKeys>,
    by_key: HashMap<String, usize>,
    resolutions: Vec<Resolution>,
    histories: Vec<Vec<HistoryEvent>>,
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
        };
        reviews.refresh();
        reviews
    }

    fn refresh(&mut self) {
        self.resolutions = self.store.resolve(&self.findings);
        self.histories = self.store.histories(&self.findings);
    }

    pub fn contains(&self, key: &str) -> bool {
        self.by_key.contains_key(key)
    }

    pub fn resolution(&self, key: &str) -> Option<&Resolution> {
        self.by_key.get(key).map(|index| &self.resolutions[*index])
    }

    /// Oldest first.
    pub fn history(&self, key: &str) -> &[HistoryEvent] {
        self.by_key
            .get(key)
            .map_or(&[], |index| self.histories[*index].as_slice())
    }

    /// Every finding's key with each of its history events.
    pub fn events(&self) -> impl Iterator<Item = (&str, &HistoryEvent)> {
        self.findings
            .iter()
            .zip(&self.histories)
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
