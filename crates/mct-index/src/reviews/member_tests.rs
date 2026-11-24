//! Per-member review of duplicate groups (issue #263, ADR-049).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use super::format::{parse_canonical, render_canonical};
use super::identity::{member_findings, MemberRow};
use super::*;

const NOW: u64 = 1_800_000_000_000;

fn hex(c: char) -> String {
    c.to_string().repeat(64)
}

fn group(identity: char, fingerprint: char) -> FindingKeys {
    FindingKeys {
        kind: Kind::Duplicate,
        identity: hex(identity),
        fingerprint: hex(fingerprint),
        content_known: true,
        legacy_key: None,
        draft_identity: hex(identity),
        group: None,
    }
}

/// Group 0 with members `paths`, each `f` in `src/<path>.rs` lines 1-9.
fn rows(paths: &[&'static str]) -> Vec<MemberRow<'static>> {
    paths
        .iter()
        .map(|p| (0, "rust", *p, "f", "function", 1, 9))
        .collect()
}

/// The group (first) and its members, as callers pass them.
fn findings(group_fp: char, paths: &[&'static str]) -> Vec<FindingKeys> {
    let groups = vec![group('1', group_fp)];
    let members = member_findings(&rows(paths), &groups);
    groups.into_iter().chain(members).collect()
}

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".mct-index")).unwrap();
    root
}

fn change(state: State, note: &str, revision: &str) -> Change {
    Change {
        state,
        note: note.to_string(),
        expected_revision: revision.to_string(),
    }
}

fn save(root: &std::path::Path, all: &[FindingKeys], target: usize, state: State, at: u64) {
    let mut reviews = Reviews::load(root);
    let revision = reviews.resolve(all)[target].revision();
    reviews
        .save(all, target, &change(state, "", &revision), at)
        .unwrap();
}

#[test]
fn members_get_their_own_keys_tied_to_the_group() {
    let all = findings('f', &["a.rs", "b.rs", "c.rs"]);
    let members = &all[1..];
    assert!(members.iter().all(|m| m.kind == Kind::DuplicateMember));
    assert!(members.iter().all(|m| m.group == Some(0)));
    assert!(members
        .iter()
        .all(|m| m.legacy_key.is_none() && m.content_known));
    let mut identities: Vec<&str> = members.iter().map(|m| m.identity.as_str()).collect();
    identities.dedup();
    assert_eq!(identities.len(), 3);
    assert!(identities.iter().all(|id| is_review_key(id)));

    // A member's identity survives a change of the group; its fingerprint
    // does not, so its decision is shown for re-evaluation.
    let changed = findings('e', &["a.rs", "b.rs", "c.rs", "d.rs"]);
    assert_eq!(changed[1].identity, all[1].identity);
    assert_ne!(changed[1].fingerprint, all[1].fingerprint);
}

#[test]
fn a_group_decision_applies_to_every_member_with_its_history() {
    let root = project();
    let all = findings('f', &["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"]);
    save(root.path(), &all, 0, State::FalsePositive, NOW);

    let reviews = Reviews::load(root.path());
    let resolutions = reviews.resolve(&all);
    for member in &resolutions[1..] {
        assert_eq!(member.state(), State::FalsePositive);
        assert_eq!(member.visibility(), Visibility::Suppressed);
        assert_eq!(member.revision(), resolutions[0].revision());
        assert!(!member.conflict);
    }
    let histories = reviews.histories(&all);
    assert_eq!(histories[3].len(), 1);
    assert_eq!(histories[3][0].to, State::FalsePositive);
    // The file holds no member row: it stays readable by older versions.
    let text = fs::read_to_string(root.path().join(".mct-index").join(CANONICAL_FILE)).unwrap();
    assert!(text.starts_with("version: 3\n"), "{text}");
}

#[test]
fn deciding_one_member_leaves_the_others_unchanged() {
    let root = project();
    let all = findings('f', &["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"]);
    let mut reviews = Reviews::load(root.path());
    let revision = reviews.resolve(&all)[2].revision();
    reviews
        .save(
            &all,
            2,
            &change(State::Confirmed, "real copy", &revision),
            NOW,
        )
        .unwrap();

    let resolutions = Reviews::load(root.path()).resolve(&all);
    assert_eq!(resolutions[2].state(), State::Confirmed);
    assert_eq!(resolutions[2].winner.as_ref().unwrap().note, "real copy");
    for other in [0, 1, 3, 4, 5] {
        assert_eq!(resolutions[other].state(), State::Unreviewed, "{other}");
    }
    let text = fs::read_to_string(root.path().join(".mct-index").join(CANONICAL_FILE)).unwrap();
    assert!(text.starts_with("version: 4\n"), "{text}");
}

#[test]
fn the_later_of_the_group_and_the_member_decision_wins() {
    let root = project();
    let all = findings('f', &["a.rs", "b.rs"]);
    save(root.path(), &all, 1, State::Confirmed, NOW);
    save(root.path(), &all, 0, State::Dismissed, NOW + 10);
    let resolutions = Reviews::load(root.path()).resolve(&all);
    assert_eq!(
        resolutions[1].state(),
        State::Dismissed,
        "apply to all wins"
    );
    save(root.path(), &all, 2, State::InReview, NOW + 20);
    let resolutions = Reviews::load(root.path()).resolve(&all);
    assert_eq!(resolutions[1].state(), State::Dismissed);
    assert_eq!(resolutions[2].state(), State::InReview);
}

#[test]
fn a_changed_group_keeps_member_decisions_for_re_evaluation_and_new_members_start_pending() {
    let root = project();
    let before = findings('f', &["a.rs", "b.rs"]);
    save(root.path(), &before, 1, State::FalsePositive, NOW);
    let after = findings('e', &["a.rs", "b.rs", "c.rs"]);
    let resolutions = Reviews::load(root.path()).resolve(&after);
    assert_eq!(resolutions[1].state(), State::FalsePositive);
    assert!(resolutions[1].needs_reevaluation());
    assert_eq!(resolutions[1].visibility(), Visibility::Actionable);
    assert_eq!(resolutions[3].state(), State::Unreviewed);
    let summary = summarize(&resolutions[1..]);
    assert_eq!(summary.label, "needs-reevaluation");
}

#[test]
fn the_group_summary_shows_the_most_pending_member_and_hides_only_when_all_are_hidden() {
    let root = project();
    let all = findings('f', &["a.rs", "b.rs", "c.rs"]);
    save(root.path(), &all, 1, State::FalsePositive, NOW);
    save(root.path(), &all, 2, State::Confirmed, NOW + 1);
    let resolutions = Reviews::load(root.path()).resolve(&all);
    let summary = summarize(&resolutions[1..]);
    assert_eq!(
        (summary.label, summary.reviewed, summary.members),
        ("unreviewed", 2, 3)
    );
    assert_eq!(summary.visibility, Visibility::Actionable);

    save(root.path(), &all, 3, State::Dismissed, NOW + 2);
    let resolutions = Reviews::load(root.path()).resolve(&all);
    let summary = summarize(&resolutions[1..]);
    assert_eq!((summary.label, summary.reviewed), ("confirmed", 3));
    assert_eq!(summary.visibility, Visibility::Actionable);

    save(root.path(), &all, 2, State::FalsePositive, NOW + 3);
    let resolutions = Reviews::load(root.path()).resolve(&all);
    let summary = summarize(&resolutions[1..]);
    assert_eq!(summary.label, "dismissed");
    assert_eq!(summary.visibility, Visibility::Suppressed);
}

#[test]
fn member_rows_need_version_4_and_round_trip() {
    let root = project();
    let all = findings('f', &["a.rs"]);
    save(root.path(), &all, 1, State::Confirmed, NOW);
    let text = fs::read_to_string(root.path().join(".mct-index").join(CANONICAL_FILE)).unwrap();
    let parsed = parse_canonical(&text).unwrap();
    assert_eq!(render_canonical(&parsed), text);

    let as_v3 = text.replacen("version: 4", "version: 3", 1);
    assert_eq!(parse_canonical(&as_v3).unwrap_err().reason, "unknown kind");
    let as_v5 = text.replacen("version: 4", "version: 5", 1);
    assert_eq!(
        parse_canonical(&as_v5).unwrap_err().reason,
        format::NEWER_FORMAT
    );
}

#[test]
fn a_legacy_group_decision_applies_to_every_member() {
    let root = project();
    let mut all = findings('f', &["a.rs", "b.rs"]);
    all[0].legacy_key = Some(hex('a'));
    fs::write(
        root.path()
            .join(".mct-index")
            .join("duplicate-reviews.toon"),
        format!(
            "version: 1\nreviews[1]{{key,reason}}:\n  {},false-positive\n",
            hex('a')
        ),
    )
    .unwrap();
    let resolutions = Reviews::load(root.path()).resolve(&all);
    assert!(
        resolutions
            .iter()
            .all(|r| r.state() == State::FalsePositive),
        "{resolutions:?}"
    );
    assert_eq!(
        summarize(&resolutions[1..]).visibility,
        Visibility::Suppressed
    );
}
