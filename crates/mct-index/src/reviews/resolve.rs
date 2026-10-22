//! Resolution of current findings against every readable source.
//!
//! Each source gives at most one candidate decision per finding; the
//! candidates are totally ordered (latest time, then the less suppressing
//! outcome, then the newest format) and the first one wins. Nothing here
//! reads a file, a clock or an environment variable.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use super::format::{Canonical, Draft, Legacy};
use super::{FindingKeys, Format, Kind, State, Visibility};

/// Rows more than this many seconds after "now" are reported as future times.
const FUTURE_SLACK_SECS: u64 = 300;

/// The parsed sources; an absent file is an empty source.
#[derive(Clone, Debug, Default)]
pub(super) struct Sources {
    pub canonical: Canonical,
    pub draft: Draft,
    pub legacy: Legacy,
}

/// One source's decision about one finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub state: State,
    /// Unix seconds; 0 when the source has no time (`A1`).
    pub at: u64,
    pub note: String,
    pub format: Format,
    /// Whether the decision was made about the finding's current code.
    pub fingerprint_matches: bool,
}

impl Candidate {
    fn suppresses(&self) -> bool {
        self.fingerprint_matches && self.state.suppresses()
    }

    /// The total order of §7.6.2: greater time, then not suppressing, then
    /// the newest format.
    fn precedence(&self) -> (Reverse<u64>, bool, Format) {
        (Reverse(self.at), self.suppresses(), self.format)
    }
}

/// The review state of one current finding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resolution {
    /// The winning candidate; `None` when no source holds a decision.
    pub winner: Option<Candidate>,
    /// A losing candidate held a different state.
    pub conflict: bool,
}

impl Resolution {
    /// The stored state that applies: `Unreviewed` without a decision or
    /// after a restore.
    pub fn state(&self) -> State {
        self.winner
            .as_ref()
            .map_or(State::Unreviewed, |winner| winner.state)
    }

    /// A decision about earlier code: shown, with its previous state, until
    /// someone decides again.
    pub fn needs_reevaluation(&self) -> bool {
        self.winner
            .as_ref()
            .is_some_and(|winner| winner.state != State::Unreviewed && !winner.fingerprint_matches)
    }

    pub fn visibility(&self) -> Visibility {
        match &self.winner {
            Some(winner) if winner.suppresses() => Visibility::Suppressed,
            _ => Visibility::Actionable,
        }
    }

    /// What consumers show: a stored state, or one of the derived
    /// `needs-reevaluation` and `resolved-but-detected`.
    pub fn label(&self) -> &'static str {
        if self.needs_reevaluation() {
            "needs-reevaluation"
        } else if self.state() == State::Resolved {
            // Someone said it was fixed, but the detector still finds the
            // same code.
            "resolved-but-detected"
        } else {
            self.state().value()
        }
    }

    /// Opaque token of the winning candidate. A save is accepted only when
    /// the caller saw the current revision (optimistic concurrency).
    pub fn revision(&self) -> String {
        self.winner.as_ref().map_or_else(
            || "none".to_string(),
            |winner| {
                format!(
                    "{}:{}:{}",
                    winner.format.value(),
                    winner.state.value(),
                    winner.at
                )
            },
        )
    }
}

/// One transition in a finding's history, from any source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEvent {
    pub format: Format,
    pub from: State,
    pub to: State,
    pub at: u64,
    pub note: String,
}

/// Counts that explain a resolution without changing it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Diagnostics {
    /// Current rows in any source that match no current finding; kept, and
    /// applied again if the code comes back.
    pub orphans: usize,
    /// Findings whose losing candidates disagree with the winner.
    pub conflicts: usize,
    /// Findings for which a legacy key or draft identity names more than one
    /// current finding, so that source decides nothing for them.
    pub ambiguous: usize,
    /// `A2` keys whose history mentions a decision that has no entry.
    pub inconsistent: usize,
    /// Rows timed more than five minutes after now.
    pub future_times: usize,
}

/// Lookups over the sources for one set of current findings.
pub(super) struct Resolver<'a> {
    sources: &'a Sources,
    draft_history: HashMap<(Kind, &'a str), Vec<usize>>,
    legacy_history: HashMap<&'a str, Vec<usize>>,
    canonical_history: HashMap<(Kind, &'a str), Vec<usize>>,
    ambiguous_legacy: HashSet<&'a str>,
    ambiguous_draft: HashSet<(Kind, &'a str)>,
}

impl<'a> Resolver<'a> {
    pub(super) fn new(sources: &'a Sources, findings: &'a [FindingKeys]) -> Self {
        let mut legacy_seen: HashMap<&str, usize> = HashMap::new();
        let mut draft_seen: HashMap<(Kind, &str), usize> = HashMap::new();
        for finding in findings {
            if let Some(key) = &finding.legacy_key {
                *legacy_seen.entry(key.as_str()).or_default() += 1;
            }
            *draft_seen
                .entry((finding.kind, finding.draft_identity.as_str()))
                .or_default() += 1;
        }
        let mut draft_history: HashMap<(Kind, &str), Vec<usize>> = HashMap::new();
        for (position, row) in sources.draft.history.iter().enumerate() {
            draft_history
                .entry((row.kind, row.identity.as_str()))
                .or_default()
                .push(position);
        }
        let mut legacy_history: HashMap<&str, Vec<usize>> = HashMap::new();
        for (position, event) in sources.legacy.history.iter().enumerate() {
            legacy_history
                .entry(event.key.as_str())
                .or_default()
                .push(position);
        }
        let mut canonical_history: HashMap<(Kind, &str), Vec<usize>> = HashMap::new();
        for (position, event) in sources.canonical.history.iter().enumerate() {
            canonical_history
                .entry((event.kind, event.identity.as_str()))
                .or_default()
                .push(position);
        }
        Self {
            sources,
            draft_history,
            legacy_history,
            canonical_history,
            ambiguous_legacy: legacy_seen
                .into_iter()
                .filter(|(_, count)| *count > 1)
                .map(|(key, _)| key)
                .collect(),
            ambiguous_draft: draft_seen
                .into_iter()
                .filter(|(_, count)| *count > 1)
                .map(|(key, _)| key)
                .collect(),
        }
    }

    fn is_ambiguous(&self, finding: &FindingKeys) -> bool {
        finding
            .legacy_key
            .as_deref()
            .is_some_and(|key| self.ambiguous_legacy.contains(key))
            || self
                .ambiguous_draft
                .contains(&(finding.kind, finding.draft_identity.as_str()))
    }

    /// Every source's candidate for `finding`, unordered.
    fn candidates(&self, finding: &FindingKeys) -> Vec<Candidate> {
        let mut candidates = Vec::with_capacity(3);
        let known = finding.content_known;
        if let Some(row) = self
            .sources
            .canonical
            .reviews
            .get(&(finding.kind, finding.identity.clone()))
        {
            candidates.push(Candidate {
                state: row.state,
                at: row.updated,
                note: row.note.clone(),
                format: Format::C3,
                fingerprint_matches: known && row.fingerprint == finding.fingerprint,
            });
        }
        let draft_key = (finding.kind, finding.draft_identity.as_str());
        if !self.ambiguous_draft.contains(&draft_key) {
            if let Some(row) = self
                .sources
                .draft
                .reviews
                .get(&(finding.kind, finding.draft_identity.clone()))
            {
                candidates.push(Candidate {
                    state: row.state,
                    at: row.at,
                    note: row.reason.clone(),
                    format: Format::B2,
                    fingerprint_matches: known && row.fingerprint == finding.fingerprint,
                });
            }
        }
        if let Some(key) = finding.legacy_key.as_deref() {
            if !self.ambiguous_legacy.contains(key) {
                candidates.extend(self.legacy_candidate(key));
            }
        }
        candidates
    }

    /// The `A1`/`A2` entry for `key`, or an `A2` restore tombstone. Legacy
    /// keys include line ranges and file hashes, so they always match.
    fn legacy_candidate(&self, key: &str) -> Option<Candidate> {
        let legacy = &self.sources.legacy;
        let format = if legacy.version == 1 {
            Format::A1
        } else {
            Format::A2
        };
        if let Some(entry) = legacy.entries.get(key) {
            return Some(Candidate {
                state: entry.state,
                at: entry.updated,
                note: entry.note.clone(),
                format,
                fingerprint_matches: true,
            });
        }
        let last = *self.legacy_history.get(key)?.last()?;
        let event = &legacy.history[last];
        (event.to == State::Unreviewed).then(|| Candidate {
            state: State::Unreviewed,
            at: event.at,
            note: event.note.clone(),
            format,
            fingerprint_matches: true,
        })
    }

    pub(super) fn resolve(&self, finding: &FindingKeys) -> Resolution {
        let mut candidates = self.candidates(finding);
        candidates.sort_by_key(Candidate::precedence);
        let mut candidates = candidates.into_iter();
        let Some(winner) = candidates.next() else {
            return Resolution::default();
        };
        let conflict = candidates.any(|loser| loser.state != winner.state);
        Resolution {
            winner: Some(winner),
            conflict,
        }
    }

    /// Every recorded transition of `finding`, oldest first: sorted by time,
    /// then format, then position in its file. Rows from different sources
    /// are distinct events and are all kept.
    pub(super) fn history(&self, finding: &FindingKeys) -> Vec<HistoryEvent> {
        let mut events: Vec<(HistoryEvent, usize)> = Vec::new();
        let canonical = &self.sources.canonical.history;
        for &position in self
            .canonical_history
            .get(&(finding.kind, finding.identity.as_str()))
            .into_iter()
            .flatten()
        {
            let event = &canonical[position];
            events.push((
                HistoryEvent {
                    format: Format::C3,
                    from: event.from,
                    to: event.to,
                    at: event.at,
                    note: event.note.clone(),
                },
                position,
            ));
        }
        let draft_key = (finding.kind, finding.draft_identity.as_str());
        if !self.ambiguous_draft.contains(&draft_key) {
            // Draft rows record only the new status; the previous one is the
            // row before it for the same identity.
            let mut from = State::Unreviewed;
            for &position in self.draft_history.get(&draft_key).into_iter().flatten() {
                let row = &self.sources.draft.history[position];
                events.push((
                    HistoryEvent {
                        format: Format::B2,
                        from,
                        to: row.state,
                        at: row.at,
                        note: row.reason.clone(),
                    },
                    position,
                ));
                from = row.state;
            }
        }
        if let Some(key) = finding.legacy_key.as_deref() {
            if !self.ambiguous_legacy.contains(key) {
                for &position in self.legacy_history.get(key).into_iter().flatten() {
                    let event = &self.sources.legacy.history[position];
                    events.push((
                        HistoryEvent {
                            format: Format::A2,
                            from: event.from,
                            to: event.to,
                            at: event.at,
                            note: event.note.clone(),
                        },
                        position,
                    ));
                }
            }
        }
        events.sort_by(|(a, a_position), (b, b_position)| {
            a.at.cmp(&b.at)
                .then(a.format.cmp(&b.format))
                .then(a_position.cmp(b_position))
        });
        events.into_iter().map(|(event, _)| event).collect()
    }

    /// The greatest time recorded anywhere for `finding`.
    pub(super) fn latest_time(&self, finding: &FindingKeys) -> u64 {
        let candidates = self.candidates(finding).into_iter().map(|c| c.at);
        let history = self.history(finding).into_iter().map(|e| e.at);
        candidates.chain(history).max().unwrap_or(0)
    }

    pub(super) fn diagnostics(&self, findings: &[FindingKeys], now: u64) -> Diagnostics {
        let canonical: HashSet<(Kind, &str)> = findings
            .iter()
            .map(|f| (f.kind, f.identity.as_str()))
            .collect();
        let draft: HashSet<(Kind, &str)> = findings
            .iter()
            .map(|f| (f.kind, f.draft_identity.as_str()))
            .collect();
        let legacy: HashSet<&str> = findings
            .iter()
            .filter_map(|f| f.legacy_key.as_deref())
            .collect();
        let sources = self.sources;
        let orphans = sources
            .canonical
            .reviews
            .keys()
            .filter(|(kind, identity)| !canonical.contains(&(*kind, identity.as_str())))
            .count()
            + sources
                .draft
                .reviews
                .keys()
                .filter(|(kind, identity)| !draft.contains(&(*kind, identity.as_str())))
                .count()
            + sources
                .legacy
                .entries
                .keys()
                .filter(|key| !legacy.contains(key.as_str()))
                .count();
        let inconsistent = self
            .legacy_history
            .iter()
            .filter(|(key, positions)| {
                !sources.legacy.entries.contains_key(**key)
                    && positions
                        .last()
                        .is_some_and(|last| sources.legacy.history[*last].to != State::Unreviewed)
            })
            .count();
        let limit = now.saturating_add(FUTURE_SLACK_SECS);
        let future_times = sources
            .canonical
            .reviews
            .values()
            .map(|row| row.updated)
            .chain(sources.canonical.history.iter().map(|event| event.at))
            .chain(sources.draft.reviews.values().map(|row| row.at))
            .chain(sources.draft.history.iter().map(|row| row.at))
            .chain(sources.legacy.entries.values().map(|entry| entry.updated))
            .chain(sources.legacy.history.iter().map(|event| event.at))
            .filter(|at| *at > limit)
            .count();
        let mut conflicts = 0;
        let mut ambiguous = 0;
        for finding in findings {
            conflicts += usize::from(self.resolve(finding).conflict);
            ambiguous += usize::from(self.is_ambiguous(finding));
        }
        Diagnostics {
            orphans,
            conflicts,
            ambiguous,
            inconsistent,
            future_times,
        }
    }
}
