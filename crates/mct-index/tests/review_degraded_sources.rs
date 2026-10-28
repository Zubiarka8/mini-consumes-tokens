//! A review file that cannot be read must never make a finding disappear.
//!
//! The canonical `review-decisions.toon` (C3) holds the decisions people make.
//! The dashboard draft `reviews.toon` (B2) and `duplicate-reviews.toon` (A) are
//! read-only bridges. When only a bridge is broken, the C3 decisions stay
//! visible but suppress nothing, because the unread file may hold a later
//! decision. Every save is refused until the file is repaired or removed.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use mct_index::reviews::{
    Change, FindingKeys, Kind, ReviewError, Reviews, State, Visibility, CANONICAL_FILE,
};

fn hex(seed: &str) -> String {
    hmac_sha256::Hash::hash(seed.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn finding(id: &str) -> FindingKeys {
    FindingKeys {
        kind: Kind::DeadCode,
        identity: hex(&format!("c3-{id}")),
        fingerprint: hex(&format!("fp-{id}")),
        content_known: true,
        legacy_key: None,
        draft_identity: hex(id),
    }
}

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".mct-index")).unwrap();
    root
}

fn b2_row(id: &str, status: &str, seconds: u64, reason: &str) -> String {
    format!(
        "  dead,{},{},{status},{seconds},{reason}\n",
        hex(id),
        hex(&format!("fp-{id}")),
    )
}

fn write_b2(root: &Path, rows: &[String]) {
    let mut text = format!(
        "version: 2\nreviews[{}]{{kind,identity,fingerprint,status,at,reason}}:\n",
        rows.len()
    );
    rows.iter().for_each(|row| text.push_str(row));
    text.push_str("history[0]{kind,identity,fingerprint,status,at,reason}:\n");
    fs::write(root.join(".mct-index/reviews.toon"), text).unwrap();
}

/// A C3 decision made through the real save path.
fn decide(root: &Path, id: &str, state: State, now: u64) {
    let findings = vec![finding(id)];
    let mut reviews = Reviews::load(root);
    let revision = reviews.resolve(&findings)[0].revision();
    let change = Change {
        state,
        note: "c3".into(),
        expected_revision: revision,
    };
    reviews.save(&findings, 0, &change, now).unwrap();
}

fn shown(reviews: &Reviews, id: &str) -> (&'static str, bool) {
    let resolution = &reviews.resolve(&[finding(id)])[0];
    (
        resolution.label(),
        resolution.visibility() == Visibility::Suppressed,
    )
}

fn try_save(reviews: &mut Reviews, id: &str) -> Result<(), ReviewError> {
    let change = Change {
        state: State::Confirmed,
        note: String::new(),
        expected_revision: "none".into(),
    };
    reviews.save(&[finding(id)], 0, &change, 9_000_000)
}

fn c3_bytes(root: &Path) -> Vec<u8> {
    fs::read(root.join(".mct-index").join(CANONICAL_FILE)).unwrap()
}

fn entries(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join(".mct-index"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

const LONG_REASON: usize = 501;

#[test]
fn healthy_c3_and_b2_both_apply_and_saving_works() {
    let root = project();
    decide(root.path(), "x", State::FalsePositive, 5_000_000);
    write_b2(root.path(), &[b2_row("y", "false_positive", 10, "bridge")]);
    let mut reviews = Reviews::load(root.path());
    assert!(!reviews.is_degraded(), "{:?}", reviews.problems());
    assert_eq!(shown(&reviews, "x"), ("false-positive", true));
    assert_eq!(shown(&reviews, "y"), ("false-positive", true));
    assert!(try_save(&mut reviews, "z").is_ok());
}

#[test]
fn a_broken_bridge_keeps_c3_decisions_visible_but_suppressing_nothing() {
    let root = project();
    decide(root.path(), "x", State::FalsePositive, 5_000_000);
    write_b2(
        root.path(),
        &[b2_row("y", "confirmed", 10, &"r".repeat(LONG_REASON))],
    );
    let reviews = Reviews::load(root.path());
    assert!(reviews.is_degraded());
    assert!(reviews.problems()[0].path.ends_with("reviews.toon"));
    // The decision is not lost: its state is reported. It just hides nothing.
    assert_eq!(shown(&reviews, "x"), ("false-positive", false));
}

#[test]
fn a_damaged_bridge_may_hold_a_later_decision_so_c3_cannot_suppress() {
    // C3 says false-positive at 5 s. The bridge holds a LATER "confirmed" that
    // cannot be read. Applying C3 would hide a finding a person reactivated.
    let root = project();
    decide(root.path(), "x", State::FalsePositive, 5_000);
    write_b2(
        root.path(),
        &[b2_row("x", "confirmed", 9_999, &"r".repeat(LONG_REASON))],
    );
    let degraded = Reviews::load(root.path());
    assert!(
        !shown(&degraded, "x").1,
        "must stay actionable while damaged"
    );
    // Repaired, the later bridge decision is what applies.
    write_b2(root.path(), &[b2_row("x", "confirmed", 9_999, "fine")]);
    let repaired = Reviews::load(root.path());
    assert!(!repaired.is_degraded());
    assert_eq!(shown(&repaired, "x"), ("confirmed", false));
}

#[test]
fn every_broken_bridge_shape_is_held_with_a_located_problem() {
    let row = b2_row("x", "false_positive", 1, "ok");
    let header = "version: 2\nreviews[2]{kind,identity,fingerprint,status,at,reason}:\n";
    let tail = "history[0]{kind,identity,fingerprint,status,at,reason}:\n";
    let cases = [
        ("duplicate identity", format!("{header}{row}{row}{tail}")),
        (
            "unknown status",
            format!("{header}{row}{}{tail}", b2_row("q", "maybe", 1, "ok")),
        ),
        ("truncated table", format!("{header}{row}")),
        ("invalid reason", {
            let long = b2_row("y", "ignored", 1, &"r".repeat(LONG_REASON));
            format!("{header}{row}{long}{tail}")
        }),
    ];
    for (name, text) in cases {
        let root = project();
        decide(root.path(), "c", State::Dismissed, 5_000);
        fs::write(root.path().join(".mct-index/reviews.toon"), &text).unwrap();
        let reviews = Reviews::load(root.path());
        assert!(reviews.is_degraded(), "{name}");
        assert!(reviews.problems()[0].reason.starts_with("line"), "{name}");
        assert_eq!(shown(&reviews, "c"), ("dismissed", false), "{name}");
        assert!(
            shown(&reviews, "x") == ("unreviewed", false),
            "{name}: rows of the broken file must not apply"
        );
    }
}

#[test]
fn an_unreadable_bridge_and_a_broken_legacy_file_are_held_too() {
    let root = project();
    decide(root.path(), "c", State::FalsePositive, 5_000);
    fs::create_dir(root.path().join(".mct-index/reviews.toon")).unwrap();
    let reviews = Reviews::load(root.path());
    assert!(reviews.problems()[0].reason.contains("not a regular file"));
    assert_eq!(shown(&reviews, "c"), ("false-positive", false));
    fs::remove_dir(root.path().join(".mct-index/reviews.toon")).unwrap();
    fs::write(
        root.path().join(".mct-index/duplicate-reviews.toon"),
        "version: 9\n",
    )
    .unwrap();
    let reviews = Reviews::load(root.path());
    assert!(reviews.is_degraded());
    assert_eq!(shown(&reviews, "c"), ("false-positive", false));
}

#[test]
fn a_broken_c3_discards_everything_even_a_valid_bridge() {
    let root = project();
    write_b2(root.path(), &[b2_row("y", "false_positive", 10, "bridge")]);
    fs::write(
        root.path().join(".mct-index").join(CANONICAL_FILE),
        "version: 3\nreviews[1]{kind,identity,fingerprint,state,updated,note}:\n",
    )
    .unwrap();
    let reviews = Reviews::load(root.path());
    assert!(reviews.is_degraded());
    assert_eq!(shown(&reviews, "y"), ("unreviewed", false));
}

#[test]
fn an_absent_bridge_is_normal() {
    let root = project();
    decide(root.path(), "x", State::Dismissed, 5_000);
    let reviews = Reviews::load(root.path());
    assert!(!reviews.is_degraded());
    assert_eq!(shown(&reviews, "x"), ("dismissed", true));
}

#[test]
fn saves_are_refused_while_a_bridge_is_broken_and_change_nothing() {
    let root = project();
    decide(root.path(), "x", State::FalsePositive, 5_000);
    write_b2(
        root.path(),
        &[b2_row("y", "confirmed", 10, &"r".repeat(LONG_REASON))],
    );
    let before = (c3_bytes(root.path()), entries(root.path()));
    let mut reviews = Reviews::load(root.path());
    for id in ["x", "y", "new"] {
        assert!(matches!(
            try_save(&mut reviews, id),
            Err(ReviewError::Degraded(_))
        ));
    }
    assert_eq!((c3_bytes(root.path()), entries(root.path())), before);
}

#[test]
fn repairing_or_removing_the_bridge_restores_suppression_and_saving() {
    let root = project();
    decide(root.path(), "x", State::FalsePositive, 5_000);
    let broken = root.path().join(".mct-index/reviews.toon");
    fs::write(&broken, "version: 2\ntruncated").unwrap();
    assert!(!shown(&Reviews::load(root.path()), "x").1);
    write_b2(root.path(), &[b2_row("y", "ignored", 10, "ok")]);
    let mut repaired = Reviews::load(root.path());
    assert_eq!(shown(&repaired, "x"), ("false-positive", true));
    assert_eq!(shown(&repaired, "y"), ("dismissed", true));
    assert!(try_save(&mut repaired, "z").is_ok());
    fs::write(&broken, "version: 2\ntruncated").unwrap();
    fs::remove_file(&broken).unwrap();
    assert_eq!(
        shown(&Reviews::load(root.path()), "x"),
        ("false-positive", true)
    );
}

#[test]
fn a_bridge_row_from_the_future_wins_until_a_later_c3_save_and_is_reported() {
    let root = project();
    decide(root.path(), "x", State::Confirmed, 5_000_000);
    let future = 9_999_999_999;
    write_b2(
        root.path(),
        &[b2_row("x", "false_positive", future, "export")],
    );
    let reviews = Reviews::load(root.path());
    // Precedence follows the event time, not the file that was read last.
    assert_eq!(shown(&reviews, "x"), ("false-positive", true));
    let diagnostics = reviews.diagnostics(&[finding("x")], 6_000_000);
    assert_eq!(diagnostics.future_times, 1);
    // A person decides again: the new C3 decision is forced past the future row.
    decide(root.path(), "x", State::Confirmed, 6_000_000);
    let after = Reviews::load(root.path());
    assert_eq!(shown(&after, "x"), ("confirmed", false));
    let winner = after.resolve(&[finding("x")])[0].winner.clone().unwrap();
    assert_eq!(winner.at, future * 1000 + 1);
}

#[test]
fn contradictory_readable_sources_resolve_by_time_and_only_a_suppressing_winner_is_held() {
    let root = project();
    let broken = root.path().join(".mct-index/duplicate-reviews.toon");
    // C3 says confirmed at 5000 s; the readable bridge says false positive at 7000 s.
    decide(root.path(), "late-bridge", State::Confirmed, 5_000_000);
    decide(root.path(), "late-confirm", State::FalsePositive, 5_000_000);
    write_b2(
        root.path(),
        &[
            b2_row("late-bridge", "false_positive", 7_000, "later"),
            // C3 false positive at 5000 s; the bridge confirms it later, at 6000 s.
            b2_row("late-confirm", "confirmed", 6_000, "later"),
        ],
    );
    fs::write(&broken, "version: 2\ntruncated").unwrap();
    let reviews = Reviews::load(root.path());
    assert!(reviews.is_degraded());
    let held = &reviews.resolve(&[finding("late-bridge")])[0];
    assert!(
        held.held && held.conflict,
        "the later bridge row wins and would hide it"
    );
    assert_eq!(held.visibility(), Visibility::Actionable);
    assert_eq!(held.effective_label(), "needs-reevaluation");
    assert_eq!(
        held.state(),
        State::FalsePositive,
        "the stored state is kept"
    );
    let confirmed = &reviews.resolve(&[finding("late-confirm")])[0];
    assert_eq!(confirmed.label(), "confirmed");
    assert!(
        !confirmed.held && confirmed.conflict,
        "a winner that hides nothing needs no hold"
    );
    // Removing the unreadable file lets the later bridge decision take effect.
    fs::remove_file(&broken).unwrap();
    let healed = Reviews::load(root.path());
    assert!(!healed.is_degraded());
    assert_eq!(shown(&healed, "late-bridge"), ("false-positive", true));
    assert_eq!(shown(&healed, "late-confirm"), ("confirmed", false));
}

#[test]
fn an_empty_canonical_file_never_lets_a_bridge_suppress() {
    let root = project();
    write_b2(root.path(), &[b2_row("x", "false_positive", 10, "bridge")]);
    fs::write(root.path().join(".mct-index").join(CANONICAL_FILE), "").unwrap();
    let mut reviews = Reviews::load(root.path());
    let suppressed = shown(&reviews, "x").1;
    let degraded = reviews.is_degraded();
    // Either the empty file is a readable empty store (nothing it says
    // overrides the bridge) or it is a problem that drops every source.
    assert!(
        degraded != suppressed,
        "degraded={degraded} suppressed={suppressed}"
    );
    if degraded {
        assert!(try_save(&mut reviews, "z").is_err());
    }
}
