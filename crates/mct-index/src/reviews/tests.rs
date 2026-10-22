#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

use super::format::{parse_canonical, render_canonical, Canonical, Event, Row};
use super::*;

/// Unix seconds, as the older formats store times.
const NOW_SECS: u64 = 1_800_000_000;
/// Unix milliseconds, as `C3` stores them and `save` takes them.
const NOW: u64 = NOW_SECS * 1000;

fn hex(c: char) -> String {
    c.to_string().repeat(64)
}

/// A finding whose keys are made of repeated hex digits.
fn finding(
    kind: Kind,
    identity: char,
    fingerprint: char,
    legacy: char,
    draft: char,
) -> FindingKeys {
    FindingKeys {
        kind,
        identity: hex(identity),
        fingerprint: hex(fingerprint),
        content_known: true,
        legacy_key: Some(hex(legacy)),
        draft_identity: hex(draft),
    }
}

fn dup() -> FindingKeys {
    finding(Kind::Duplicate, '1', 'f', 'a', 'd')
}

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".mct-index")).unwrap();
    root
}

fn write(root: &Path, name: &str, text: &str) {
    fs::write(root.join(".mct-index").join(name), text).unwrap();
}

fn read(root: &Path, name: &str) -> Option<String> {
    fs::read_to_string(root.join(".mct-index").join(name)).ok()
}

fn canonical(rows: &[(Kind, char, char, State, u64)]) -> String {
    let mut store = Canonical::default();
    for (kind, identity, fingerprint, state, at) in rows {
        store.reviews.insert(
            (*kind, hex(*identity)),
            Row {
                kind: *kind,
                fingerprint: hex(*fingerprint),
                state: *state,
                updated: *at,
                note: String::new(),
            },
        );
    }
    render_canonical(&store)
}

fn draft(rows: &[(&str, char, char, &str, u64, &str)]) -> String {
    let lines: Vec<String> = rows
        .iter()
        .map(|(kind, identity, fingerprint, status, at, reason)| {
            format!(
                "  {kind},{},{},{status},{at},{reason}",
                hex(*identity),
                hex(*fingerprint)
            )
        })
        .collect();
    let header = "{kind,identity,fingerprint,status,at,reason}:";
    format!(
        "version: 2\nreviews[{n}]{header}\n{rows}history[{n}]{header}\n{rows}",
        n = lines.len(),
        rows = lines.iter().map(|l| format!("{l}\n")).collect::<String>()
    )
}

fn legacy_v1(rows: &[(char, &str)]) -> String {
    let mut text = format!("version: 1\nreviews[{}]{{key,reason}}:\n", rows.len());
    for (key, reason) in rows {
        text.push_str(&format!("  {},{reason}\n", hex(*key)));
    }
    text
}

/// `entries`: (key, state, updated, note); `events`: (key, from, to, at).
fn legacy_v2(entries: &[(char, &str, u64, &str)], events: &[(char, &str, &str, u64)]) -> String {
    let mut text = format!(
        "version: 2\nreviews[{}]{{key,state,updated,note}}:\n",
        entries.len()
    );
    for (key, state, updated, note) in entries {
        text.push_str(&format!("  {},{state},{updated},{note}\n", hex(*key)));
    }
    text.push_str(&format!(
        "history[{}]{{key,from,to,at,note}}:\n",
        events.len()
    ));
    for (key, from, to, at) in events {
        text.push_str(&format!("  {},{from},{to},{at},\n", hex(*key)));
    }
    text
}

fn resolve_one(root: &Path, finding: &FindingKeys) -> Resolution {
    Reviews::load(root)
        .resolve(std::slice::from_ref(finding))
        .remove(0)
}

fn change(state: State, revision: &str) -> Change {
    Change {
        state,
        note: String::new(),
        expected_revision: revision.to_string(),
    }
}

// ---------- formats ----------

#[test]
fn canonical_files_round_trip_byte_for_byte() {
    let mut store = Canonical::default();
    store.reviews.insert(
        (Kind::DeadCode, hex('1')),
        Row {
            kind: Kind::DeadCode,
            fingerprint: hex('2'),
            state: State::FalsePositive,
            updated: 7,
            note: "generated, kept: \"yes\" 100%".into(),
        },
    );
    store.history.push(Event {
        kind: Kind::DeadCode,
        identity: hex('1'),
        fingerprint: hex('2'),
        from: State::Unreviewed,
        to: State::FalsePositive,
        at: 7,
        note: "generated, kept: \"yes\" 100%".into(),
    });
    let text = render_canonical(&store);
    assert!(
        !text.contains("generated,"),
        "notes never hold raw commas: {text}"
    );
    let parsed = parse_canonical(&text).unwrap();
    assert_eq!(parsed, store);
    assert_eq!(render_canonical(&parsed), text);
}

#[test]
fn line_endings_and_field_counts_follow_the_strict_rules() {
    let key = hex('a');
    let row = format!("  dead-code,{key},{key},confirmed,1,");
    let text = format!(
        "version: 3\nreviews[1]{{kind,identity,fingerprint,state,updated,note}}:\n{row}\nhistory[0]{{kind,identity,fingerprint,from,to,at,note}}:\n"
    );
    let parsed = parse_canonical(&text).unwrap();
    // Windows line endings read as the same file.
    assert_eq!(
        parse_canonical(&text.replace('\n', "\r\n")).unwrap(),
        parsed
    );
    // A missing final newline is accepted, as `str::lines` accepts it.
    assert_eq!(parse_canonical(text.trim_end()).unwrap(), parsed);
    // A row with too few or too many fields is refused, on its own line.
    for bad in [
        format!("  dead-code,{key},{key},confirmed,1"),
        format!("  dead-code,{key},{key},confirmed,1,,"),
    ] {
        let broken = text.replace(&row, &bad);
        let error = parse_canonical(&broken).unwrap_err();
        assert_eq!((error.line, error.reason), (3, "wrong number of fields"));
    }
}

#[test]
fn malformed_files_degrade_the_whole_store_with_line_and_reason() {
    let key = hex('a');
    let cases: Vec<(&str, String)> = vec![
        ("review-decisions.toon", "version: 3\nreviews[1]{kind,identity,fingerprint,state,updated,note}:\n".into()),
        ("review-decisions.toon", format!("version: 3\nreviews[1]{{kind,identity,fingerprint,state,updated,note}}:\n  dead-code,{key},{key},bogus,1,\nhistory[0]{{kind,identity,fingerprint,from,to,at,note}}:\n")),
        ("review-decisions.toon", format!("version: 3\nreviews[1]{{kind,identity,fingerprint,state,updated,note}}:\n  dead-code,{key},{key},confirmed,-1,\nhistory[0]{{kind,identity,fingerprint,from,to,at,note}}:\n")),
        ("review-decisions.toon", format!("version: 3\nreviews[1]{{kind,identity,fingerprint,state,updated,note}}:\n  dead-code,{key},{key},confirmed,1,raw note\nhistory[0]{{kind,identity,fingerprint,from,to,at,note}}:\n")),
        ("review-decisions.toon", "version: 3\nreviews[0]{kind,identity,fingerprint,state,updated,note}:\nhistory[0]{kind,identity,fingerprint,from,to,at,note}:\nextra\n".into()),
        ("review-decisions.toon", "version: 9\n".into()),
        ("review-decisions.toon", String::new()),
        ("reviews.toon", "version: 2\nreviews[1]{kind,identity,fingerprint,status,at,reason}:\n".into()),
        ("reviews.toon", format!("version: 2\nreviews[1]{{kind,identity,fingerprint,status,at,reason}}:\n  dead,{key},{key},ignored,1, padded \nhistory[0]{{kind,identity,fingerprint,status,at,reason}}:\n")),
        ("duplicate-reviews.toon", format!("version: 1\nreviews[2]{{key,reason}}:\n  {key},dismissed\n")),
        ("duplicate-reviews.toon", format!("version: 1\nreviews[1]{{key,reason}}:\n  {key},in-review-ish\n")),
        ("duplicate-reviews.toon", format!("version: 2\nreviews[0]{{key,state,updated,note}}:\nhistory[1]{{key,from,to,at,note}}:\n  {key},none,bogus,1,\n")),
        ("duplicate-reviews.toon", "version: 3\n".into()),
    ];
    for (name, text) in cases {
        let root = project();
        write(
            root.path(),
            "review-decisions.toon",
            &canonical(&[(Kind::Duplicate, '1', 'f', State::Dismissed, 5)]),
        );
        write(root.path(), name, &text);
        let reviews = Reviews::load(root.path());
        assert!(reviews.is_degraded(), "{name}: {text}");
        let problem = &reviews.problems()[0];
        assert!(problem.path.ends_with(name), "{problem:?}");
        assert!(!problem.reason.is_empty());
        // Nothing hides, even the valid canonical decision.
        let resolution = reviews.resolve(&[dup()]).remove(0);
        assert_eq!(resolution, Resolution::default(), "{name}: {text}");
    }
}

#[test]
fn a_newer_canonical_version_is_reported_as_such() {
    let root = project();
    write(
        root.path(),
        "review-decisions.toon",
        "version: 4\nanything\n",
    );
    let reviews = Reviews::load(root.path());
    assert!(reviews.problems()[0].newer_format);
}

#[test]
fn oversized_files_are_rejected_before_parsing() {
    let root = project();
    let path = root.path().join(".mct-index/reviews.toon");
    fs::write(&path, vec![b'a'; 32 * 1024 * 1024 + 1]).unwrap();
    let reviews = Reviews::load(root.path());
    assert!(reviews.problems()[0].reason.contains("32 MiB"));
}

#[test]
fn a_line_that_is_not_utf8_degrades_the_store_on_its_line() {
    let root = project();
    let mut bytes = canonical(&[(Kind::Duplicate, '1', 'f', State::Dismissed, 5)]).into_bytes();
    bytes.extend_from_slice(b"\xff\n");
    fs::write(root.path().join(".mct-index/review-decisions.toon"), bytes).unwrap();
    let reviews = Reviews::load(root.path());
    assert!(reviews.is_degraded());
    assert!(
        reviews.problems()[0]
            .reason
            .starts_with("line 5: unreadable line"),
        "{:?}",
        reviews.problems()
    );
    // Nothing is hidden from a partly read file.
    assert_eq!(reviews.resolve(&[dup()]).remove(0), Resolution::default());
}

#[cfg(unix)]
#[test]
fn symlinks_and_hard_links_are_never_followed() {
    use std::os::unix::fs::symlink;
    let root = project();
    let outside = root.path().join("outside.toon");
    fs::write(&outside, legacy_v1(&[('a', "dismissed")])).unwrap();
    let file = root.path().join(".mct-index/duplicate-reviews.toon");
    symlink(&outside, &file).unwrap();
    assert!(Reviews::load(root.path()).is_degraded());
    fs::remove_file(&file).unwrap();
    fs::hard_link(&outside, &file).unwrap();
    assert!(Reviews::load(root.path()).is_degraded());
}

// ---------- legacy sources ----------

#[test]
fn every_legacy_format_resolves_with_its_state_time_and_note() {
    let root = project();
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v1(&[('a', "false-positive")]),
    );
    let r = resolve_one(root.path(), &dup());
    assert_eq!(r.state(), State::FalsePositive);
    assert_eq!(r.winner.as_ref().unwrap().format, Format::A1);
    assert_eq!(r.winner.as_ref().unwrap().at, 0);
    assert_eq!(r.visibility(), Visibility::Suppressed);

    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(
            &[('a', "dismissed", 40, "kept+for+now%2C+see+%2312")],
            &[('a', "none", "dismissed", 40)],
        ),
    );
    let r = resolve_one(root.path(), &dup());
    let winner = r.winner.clone().unwrap();
    assert_eq!(
        (winner.state, winner.at, winner.format),
        (State::Dismissed, 40_000, Format::A2)
    );
    assert_eq!(winner.note, "kept for now, see #12");

    let root = project();
    write(
        root.path(),
        "reviews.toon",
        &draft(&[("duplicate", 'd', 'f', "ignored", 50, "generated, keep")]),
    );
    let r = resolve_one(root.path(), &dup());
    let winner = r.winner.clone().unwrap();
    assert_eq!(
        (winner.state, winner.at, winner.format),
        (State::Dismissed, 50_000, Format::B2)
    );
    assert_eq!(winner.note, "generated, keep");
    assert_eq!(r.visibility(), Visibility::Suppressed);
}

#[test]
fn a_draft_decision_about_other_code_needs_reevaluation() {
    let root = project();
    write(
        root.path(),
        "reviews.toon",
        &draft(&[("duplicate", 'd', 'e', "false_positive", 50, "")]),
    );
    let r = resolve_one(root.path(), &dup());
    assert!(r.needs_reevaluation());
    assert_eq!(r.label(), "needs-reevaluation");
    assert_eq!(
        r.state(),
        State::FalsePositive,
        "the previous state is kept"
    );
    assert_eq!(r.visibility(), Visibility::Actionable);
}

#[test]
fn unknown_content_is_never_suppressed() {
    let root = project();
    write(
        root.path(),
        "review-decisions.toon",
        &canonical(&[(Kind::Duplicate, '1', 'f', State::Dismissed, 5)]),
    );
    let mut finding = dup();
    finding.content_known = false;
    finding.legacy_key = None;
    let r = resolve_one(root.path(), &finding);
    assert_eq!(r.visibility(), Visibility::Actionable);
    assert!(r.needs_reevaluation());
}

#[test]
fn ambiguous_legacy_keys_decide_nothing_for_either_finding() {
    let root = project();
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v1(&[('a', "dismissed")]),
    );
    write(
        root.path(),
        "reviews.toon",
        &draft(&[("dead", '9', 'f', "ignored", 3, "")]),
    );
    let first = dup();
    let mut second = finding(Kind::Duplicate, '2', 'e', 'a', 'c');
    second.legacy_key = first.legacy_key.clone();
    let dead_a = finding(Kind::DeadCode, '3', 'f', '5', '9');
    let dead_b = finding(Kind::DeadCode, '4', 'f', '6', '9');
    let findings = [first, second, dead_a, dead_b];
    let reviews = Reviews::load(root.path());
    for resolution in reviews.resolve(&findings) {
        assert_eq!(resolution, Resolution::default());
    }
    assert_eq!(reviews.diagnostics(&findings, NOW).ambiguous, 4);
}

#[test]
fn a2_restores_are_tombstones_and_inconsistent_keys_decide_nothing() {
    let root = project();
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(
            &[],
            &[
                ('a', "none", "dismissed", 10),
                ('a', "dismissed", "none", 20),
                ('b', "none", "confirmed", 30),
            ],
        ),
    );
    let r = resolve_one(root.path(), &dup());
    let winner = r.winner.clone().unwrap();
    assert_eq!((winner.state, winner.at), (State::Unreviewed, 20_000));
    assert_eq!(r.state(), State::Unreviewed);
    let other = finding(Kind::Duplicate, '2', 'e', 'b', 'c');
    let reviews = Reviews::load(root.path());
    assert_eq!(
        reviews.resolve(std::slice::from_ref(&other)).remove(0),
        Resolution::default()
    );
    assert_eq!(reviews.diagnostics(&[dup(), other], NOW).inconsistent, 1);
}

// ---------- conflict resolution ----------

#[test]
fn the_latest_decision_wins_across_sources() {
    let root = project();
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(&[('a', "dismissed", 100, "")], &[]),
    );
    write(
        root.path(),
        "reviews.toon",
        &draft(&[("duplicate", 'd', 'f', "confirmed", 90, "")]),
    );
    write(
        root.path(),
        "review-decisions.toon",
        &canonical(&[(Kind::Duplicate, '1', 'f', State::InReview, 80)]),
    );
    let r = resolve_one(root.path(), &dup());
    assert_eq!(r.state(), State::Dismissed);
    assert!(r.conflict);
    assert_eq!(
        Reviews::load(root.path())
            .diagnostics(&[dup()], NOW)
            .conflicts,
        1
    );
}

#[test]
fn equal_times_prefer_the_visible_outcome_then_the_newest_format() {
    let root = project();
    // Same second: suppressing C3 against an actionable A2 decision.
    write(
        root.path(),
        "review-decisions.toon",
        &canonical(&[(Kind::Duplicate, '1', 'f', State::FalsePositive, 50_000)]),
    );
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(&[('a', "in-review", 50, "")], &[]),
    );
    let r = resolve_one(root.path(), &dup());
    assert_eq!(
        (r.state(), r.winner.as_ref().unwrap().format),
        (State::InReview, Format::A2)
    );
    // Both actionable: the newest format decides.
    write(
        root.path(),
        "review-decisions.toon",
        &canonical(&[(Kind::Duplicate, '1', 'f', State::Confirmed, 50_000)]),
    );
    let r = resolve_one(root.path(), &dup());
    assert_eq!(
        (r.state(), r.winner.as_ref().unwrap().format),
        (State::Confirmed, Format::C3)
    );
    // Both suppressing: the newest format decides too.
    write(
        root.path(),
        "review-decisions.toon",
        &canonical(&[(Kind::Duplicate, '1', 'f', State::Dismissed, 50_000)]),
    );
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(&[('a', "false-positive", 50, "")], &[]),
    );
    let r = resolve_one(root.path(), &dup());
    assert_eq!(
        (r.state(), r.winner.as_ref().unwrap().format),
        (State::Dismissed, Format::C3)
    );
}

#[test]
fn unknown_times_lose_to_any_known_time() {
    let root = project();
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v1(&[('a', "dismissed")]),
    );
    write(
        root.path(),
        "reviews.toon",
        &draft(&[("duplicate", 'd', 'f', "pending", 1, "")]),
    );
    assert_eq!(resolve_one(root.path(), &dup()).state(), State::Unreviewed);
}

#[test]
fn resolution_does_not_depend_on_the_order_of_findings() {
    let root = project();
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(&[('a', "dismissed", 9, ""), ('b', "confirmed", 9, "")], &[]),
    );
    write(
        root.path(),
        "review-decisions.toon",
        &canonical(&[
            (Kind::Duplicate, '1', 'f', State::InReview, 9_000),
            (Kind::DeadCode, '3', 'e', State::Dismissed, 4),
        ]),
    );
    let mut findings = vec![
        dup(),
        finding(Kind::Duplicate, '2', 'e', 'b', 'c'),
        finding(Kind::DeadCode, '3', 'e', '5', '6'),
    ];
    let reviews = Reviews::load(root.path());
    let forward = reviews.resolve(&findings);
    findings.reverse();
    let mut backward = reviews.resolve(&findings);
    backward.reverse();
    assert_eq!(forward, backward);
    assert_eq!(forward[0].state(), State::InReview);
}

// ---------- tombstones and history ----------

#[test]
fn a_canonical_tombstone_beats_older_legacy_decisions_and_loses_to_newer_ones() {
    let root = project();
    write(
        root.path(),
        "review-decisions.toon",
        &canonical(&[(Kind::Duplicate, '1', 'e', State::Unreviewed, 60_000)]),
    );
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(&[('a', "dismissed", 50, "")], &[]),
    );
    let r = resolve_one(root.path(), &dup());
    assert_eq!(r.state(), State::Unreviewed);
    assert!(
        !r.needs_reevaluation(),
        "a tombstone with another fingerprint is still a restore"
    );
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(&[('a', "dismissed", 70, "")], &[]),
    );
    assert_eq!(resolve_one(root.path(), &dup()).state(), State::Dismissed);
}

#[test]
fn history_is_the_union_of_every_source_in_time_order() {
    let root = project();
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(
            &[('a', "dismissed", 10, "")],
            &[('a', "none", "dismissed", 10)],
        ),
    );
    write(
        root.path(),
        "reviews.toon",
        &draft(&[("duplicate", 'd', 'f', "confirmed", 5, "")]),
    );
    let mut reviews = Reviews::load(root.path());
    let revision = reviews.resolve(&[dup()])[0].revision();
    reviews
        .save(&[dup()], 0, &change(State::InReview, &revision), 8)
        .unwrap();
    let history = reviews.histories(&[dup()]).remove(0);
    let summary: Vec<_> = history
        .iter()
        .map(|e| (e.format, e.from, e.to, e.at))
        .collect();
    assert_eq!(
        summary,
        vec![
            (Format::B2, State::Unreviewed, State::Confirmed, 5_000),
            (Format::A2, State::Unreviewed, State::Dismissed, 10_000),
            (Format::C3, State::Dismissed, State::InReview, 10_001),
        ]
    );
}

// ---------- saving ----------

#[test]
fn saving_writes_only_the_canonical_file_and_keeps_legacy_bytes() {
    let root = project();
    let legacy = legacy_v2(
        &[('a', "dismissed", 10, "a%2Cb")],
        &[('a', "none", "dismissed", 10)],
    );
    let draft_text = draft(&[("dead", '9', 'f', "ignored", 3, "x, y")]);
    write(root.path(), "duplicate-reviews.toon", &legacy);
    write(root.path(), "reviews.toon", &draft_text);
    let mut reviews = Reviews::load(root.path());
    let revision = reviews.resolve(&[dup()])[0].revision();
    assert_eq!(revision, "a2:dismissed:10000");
    reviews
        .save(&[dup()], 0, &change(State::Unreviewed, &revision), NOW)
        .unwrap();
    assert_eq!(read(root.path(), "duplicate-reviews.toon").unwrap(), legacy);
    assert_eq!(read(root.path(), "reviews.toon").unwrap(), draft_text);
    assert!(read(root.path(), "review-decisions.toon")
        .unwrap()
        .starts_with("version: 3\n"));
    assert_eq!(resolve_one(root.path(), &dup()).state(), State::Unreviewed);
}

#[test]
fn a_stale_revision_is_rejected_without_writing() {
    let root = project();
    let mut first = Reviews::load(root.path());
    let mut second = Reviews::load(root.path());
    first
        .save(&[dup()], 0, &change(State::Confirmed, "none"), NOW)
        .unwrap();
    let saved = read(root.path(), "review-decisions.toon").unwrap();
    let error = second
        .save(&[dup()], 0, &change(State::Dismissed, "none"), NOW)
        .unwrap_err();
    assert!(matches!(error, ReviewError::Conflict), "{error}");
    assert_eq!(read(root.path(), "review-decisions.toon").unwrap(), saved);
    // The rejected writer now sees the other writer's decision.
    assert_eq!(second.resolve(&[dup()])[0].state(), State::Confirmed);
}

#[test]
fn two_writers_on_different_findings_both_persist() {
    let root = project();
    let other = finding(Kind::DeadCode, '2', 'e', 'b', 'c');
    let findings = [dup(), other];
    let mut first = Reviews::load(root.path());
    let mut second = Reviews::load(root.path());
    first
        .save(&findings, 0, &change(State::Dismissed, "none"), NOW)
        .unwrap();
    second
        .save(&findings, 1, &change(State::Confirmed, "none"), NOW)
        .unwrap();
    let states: Vec<State> = Reviews::load(root.path())
        .resolve(&findings)
        .iter()
        .map(Resolution::state)
        .collect();
    assert_eq!(states, vec![State::Dismissed, State::Confirmed]);
}

#[test]
fn a_new_decision_is_always_later_than_everything_recorded() {
    let root = project();
    // A legacy row from the future, and a clock that went back.
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(&[('a', "dismissed", NOW_SECS + 10_000, "")], &[]),
    );
    let mut reviews = Reviews::load(root.path());
    assert_eq!(reviews.diagnostics(&[dup()], NOW).future_times, 1);
    let revision = reviews.resolve(&[dup()])[0].revision();
    reviews
        .save(&[dup()], 0, &change(State::InReview, &revision), NOW)
        .unwrap();
    let r = reviews.resolve(&[dup()]).remove(0);
    assert_eq!(r.state(), State::InReview);
    assert_eq!(r.winner.unwrap().at, (NOW_SECS + 10_000) * 1000 + 1);
    let revision = reviews.resolve(&[dup()])[0].revision();
    reviews
        .save(&[dup()], 0, &change(State::Confirmed, &revision), NOW - 5)
        .unwrap();
    let times: Vec<u64> = reviews
        .histories(&[dup()])
        .remove(0)
        .iter()
        .map(|e| e.at)
        .collect();
    assert!(times.windows(2).all(|w| w[0] < w[1]), "{times:?}");
}

#[test]
fn the_maximum_time_refuses_the_save() {
    let root = project();
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(&[('a', "dismissed", u64::MAX, "")], &[]),
    );
    let mut reviews = Reviews::load(root.path());
    let revision = reviews.resolve(&[dup()])[0].revision();
    let error = reviews
        .save(&[dup()], 0, &change(State::InReview, &revision), NOW)
        .unwrap_err();
    assert!(matches!(error, ReviewError::Full(_)), "{error}");
    assert!(read(root.path(), "review-decisions.toon").is_none());
}

#[test]
fn repeating_the_state_writes_nothing_but_re_deciding_changed_code_does() {
    let root = project();
    let mut reviews = Reviews::load(root.path());
    reviews
        .save(&[dup()], 0, &change(State::Dismissed, "none"), NOW)
        .unwrap();
    let revision = reviews.resolve(&[dup()])[0].revision();
    reviews
        .save(&[dup()], 0, &change(State::Dismissed, &revision), NOW)
        .unwrap();
    assert_eq!(reviews.histories(&[dup()])[0].len(), 1);
    // Restoring a finding without a decision is a no-op too.
    let fresh = finding(Kind::DeadCode, '7', 'e', 'b', 'c');
    reviews
        .save(
            std::slice::from_ref(&fresh),
            0,
            &change(State::Unreviewed, "none"),
            NOW,
        )
        .unwrap();
    assert_eq!(reviews.histories(std::slice::from_ref(&fresh))[0].len(), 0);
    // The code changed: confirming the same state again re-anchors it.
    let mut changed = dup();
    changed.fingerprint = hex('e');
    let r = reviews.resolve(std::slice::from_ref(&changed)).remove(0);
    assert!(r.needs_reevaluation());
    reviews
        .save(
            std::slice::from_ref(&changed),
            0,
            &change(State::Dismissed, &r.revision()),
            NOW,
        )
        .unwrap();
    let r = reviews.resolve(std::slice::from_ref(&changed)).remove(0);
    assert_eq!(r.visibility(), Visibility::Suppressed);
}

#[test]
fn a_live_lock_refuses_the_save_and_is_released_when_dropped() {
    let root = project();
    let holder = fs::File::create(root.path().join(".mct-index/review-decisions.lock")).unwrap();
    holder.try_lock().unwrap();
    let mut reviews = Reviews::load(root.path());
    let error = reviews
        .save(&[dup()], 0, &change(State::Dismissed, "none"), NOW)
        .unwrap_err();
    assert!(matches!(error, ReviewError::Locked { .. }), "{error}");
    assert!(error.to_string().contains("review-decisions.lock"));
    assert!(read(root.path(), "review-decisions.toon").is_none());
    drop(holder);
    reviews
        .save(&[dup()], 0, &change(State::Dismissed, "none"), NOW)
        .expect("the lock is free once its holder is gone");
}

#[test]
fn a_lock_file_left_by_a_killed_writer_does_not_block_saves() {
    let root = project();
    // The file survives a crash; nothing holds it any more.
    write(root.path(), "review-decisions.lock", "pid 1");
    let mut reviews = Reviews::load(root.path());
    reviews
        .save(&[dup()], 0, &change(State::Dismissed, "none"), NOW)
        .expect("a killed writer must not block the next one");
    assert_eq!(resolve_one(root.path(), &dup()).state(), State::Dismissed);
}

#[test]
fn a_save_removes_only_the_temporary_files_this_storage_created() {
    let root = project();
    let own = "review-decisions.toon.review-decisions.toon.1.2.0.tmp";
    let own_backup = "review-decisions.toon.review-decisions.toon.prev.1.2.0.tmp";
    write(root.path(), own, "version: 3\npartial");
    write(root.path(), own_backup, "version: 3\npartial");
    let others = [
        "review-decisions.toon.notes.tmp",
        "review-decisions.toon.evil.1.2.0.tmp",
        "review-decisions.toon.prev",
    ];
    for name in others {
        write(root.path(), name, "kept");
    }
    let mut reviews = Reviews::load(root.path());
    assert_eq!(reviews.leftover_temporary_files(), 2);
    reviews
        .save(&[dup()], 0, &change(State::Dismissed, "none"), NOW)
        .unwrap();
    assert!(read(root.path(), own).is_none());
    assert!(read(root.path(), own_backup).is_none());
    for name in others {
        assert_eq!(read(root.path(), name).unwrap(), "kept", "{name}");
    }
    assert_eq!(Reviews::load(root.path()).leftover_temporary_files(), 0);
}

#[test]
fn a_temporary_name_that_cannot_be_removed_does_not_fail_the_save() {
    let root = project();
    let directory = root
        .path()
        .join(".mct-index/review-decisions.toon.review-decisions.toon.1.2.0.tmp");
    fs::create_dir_all(&directory).unwrap();
    let mut reviews = Reviews::load(root.path());
    assert_eq!(reviews.leftover_temporary_files(), 0, "only files count");
    reviews
        .save(&[dup()], 0, &change(State::Dismissed, "none"), NOW)
        .expect("the save succeeds");
    assert!(directory.is_dir());
    assert_eq!(resolve_one(root.path(), &dup()).state(), State::Dismissed);
}

#[test]
fn a_degraded_store_refuses_every_save() {
    let root = project();
    write(root.path(), "reviews.toon", "version: 2\ngarbage\n");
    let mut reviews = Reviews::load(root.path());
    let error = reviews
        .save(&[dup()], 0, &change(State::Dismissed, "none"), NOW)
        .unwrap_err();
    assert!(matches!(error, ReviewError::Degraded(_)), "{error}");
    assert!(read(root.path(), "review-decisions.toon").is_none());
    assert_eq!(
        read(root.path(), "reviews.toon").unwrap(),
        "version: 2\ngarbage\n"
    );
}

#[test]
fn invalid_notes_are_rejected() {
    let root = project();
    let mut reviews = Reviews::load(root.path());
    for note in ["line\nbreak".to_string(), "x".repeat(501)] {
        let request = Change {
            note,
            ..change(State::Dismissed, "none")
        };
        assert!(matches!(
            reviews.save(&[dup()], 0, &request, NOW),
            Err(ReviewError::Invalid(_))
        ));
    }
}

#[test]
fn the_previous_generation_is_kept_and_restorable() {
    let root = project();
    let mut reviews = Reviews::load(root.path());
    reviews
        .save(&[dup()], 0, &change(State::Dismissed, "none"), NOW)
        .unwrap();
    assert!(
        read(root.path(), "review-decisions.toon.prev").is_none(),
        "no earlier file"
    );
    let first = read(root.path(), "review-decisions.toon").unwrap();
    let revision = reviews.resolve(&[dup()])[0].revision();
    reviews
        .save(&[dup()], 0, &change(State::Confirmed, &revision), NOW)
        .unwrap();
    assert_eq!(
        read(root.path(), "review-decisions.toon.prev").unwrap(),
        first
    );
    // Recovery: a corrupt canonical file degrades; restoring `.prev` by hand
    // brings back the earlier decisions.
    write(
        root.path(),
        "review-decisions.toon",
        "version: 3\ntruncated",
    );
    assert!(Reviews::load(root.path()).is_degraded());
    fs::copy(
        root.path().join(".mct-index/review-decisions.toon.prev"),
        root.path().join(".mct-index/review-decisions.toon"),
    )
    .unwrap();
    assert_eq!(resolve_one(root.path(), &dup()).state(), State::Dismissed);
}

#[test]
fn an_interrupted_save_leaves_the_old_file_and_a_reported_temporary_file() {
    let root = project();
    let mut reviews = Reviews::load(root.path());
    reviews
        .save(&[dup()], 0, &change(State::Dismissed, "none"), NOW)
        .unwrap();
    let saved = read(root.path(), "review-decisions.toon").unwrap();
    // What a crash between sync and rename leaves behind.
    write(
        root.path(),
        "review-decisions.toon.review-decisions.toon.1.2.0.tmp",
        "version: 3\npartial",
    );
    let reviews = Reviews::load(root.path());
    assert!(!reviews.is_degraded());
    assert_eq!(reviews.leftover_temporary_files(), 1);
    assert_eq!(read(root.path(), "review-decisions.toon").unwrap(), saved);
    assert_eq!(resolve_one(root.path(), &dup()).state(), State::Dismissed);
}

#[test]
fn orphans_are_kept_and_counted() {
    let root = project();
    write(
        root.path(),
        "review-decisions.toon",
        &canonical(&[(Kind::DeadCode, '8', 'e', State::Dismissed, 5)]),
    );
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v1(&[('c', "dismissed")]),
    );
    let reviews = Reviews::load(root.path());
    assert_eq!(reviews.diagnostics(&[dup()], NOW).orphans, 2);
}

// ---------- policy ----------

#[test]
fn labels_and_visibility_follow_the_policy_table() {
    let candidate = |state, fingerprint_matches| Resolution {
        winner: Some(Candidate {
            state,
            at: 1,
            note: String::new(),
            format: Format::C3,
            fingerprint_matches,
        }),
        conflict: false,
    };
    let cases = [
        (Resolution::default(), "unreviewed", Visibility::Actionable),
        (
            candidate(State::Unreviewed, false),
            "unreviewed",
            Visibility::Actionable,
        ),
        (
            candidate(State::InReview, true),
            "in-review",
            Visibility::Actionable,
        ),
        (
            candidate(State::Confirmed, true),
            "confirmed",
            Visibility::Actionable,
        ),
        (
            candidate(State::FalsePositive, true),
            "false-positive",
            Visibility::Suppressed,
        ),
        (
            candidate(State::Dismissed, true),
            "dismissed",
            Visibility::Suppressed,
        ),
        (
            candidate(State::Resolved, true),
            "resolved-but-detected",
            Visibility::Actionable,
        ),
        (
            candidate(State::Dismissed, false),
            "needs-reevaluation",
            Visibility::Actionable,
        ),
        (
            candidate(State::Resolved, false),
            "needs-reevaluation",
            Visibility::Actionable,
        ),
    ];
    for (resolution, label, visibility) in cases {
        assert_eq!(
            (resolution.label(), resolution.visibility()),
            (label, visibility)
        );
    }
}

// ---------- identity ----------

fn symbol(path: &str, name: &str, kind: &str, line: u32, end: u32) -> crate::SymbolListEntry {
    crate::SymbolListEntry {
        name: name.into(),
        kind: kind.into(),
        language: "rust".into(),
        relative_path: path.into(),
        line,
        end_line: Some(end),
        parent: None,
        level: None,
    }
}

fn hashes() -> Vec<(String, String)> {
    vec![("a.rs".into(), "h1".into()), ("b.rs".into(), "h2".into())]
}

#[test]
fn dead_code_identity_survives_edits_and_a_sibling_gaining_a_reference() {
    let symbols = vec![
        symbol("a.rs", "parse", "function", 1, 3),
        symbol("a.rs", "parse", "function", 10, 12),
        symbol("a.rs", "other", "function", 20, 22),
    ];
    let both = dead_code_findings(&symbols, &[0, 1], &hashes());
    // The first `parse` gains a reference: only the second is a candidate.
    let one = dead_code_findings(&symbols, &[1], &hashes());
    assert_eq!(one[0].identity, both[1].identity);
    assert_eq!(one[0].fingerprint, both[1].fingerprint);
    assert_ne!(both[0].identity, both[1].identity);
    // The draft identity shifts, which the fingerprint catches.
    assert_eq!(one[0].draft_identity, both[0].draft_identity);
    // Lines inserted above: same identity, new fingerprint and legacy key.
    let moved = vec![
        symbol("a.rs", "parse", "function", 2, 4),
        symbol("a.rs", "parse", "function", 11, 13),
        symbol("a.rs", "other", "function", 21, 23),
    ];
    let after = dead_code_findings(&moved, &[0, 1], &hashes());
    assert_eq!(after[0].identity, both[0].identity);
    assert_ne!(after[0].fingerprint, both[0].fingerprint);
    assert_ne!(after[0].legacy_key, both[0].legacy_key);
}

#[test]
fn identical_files_at_two_paths_have_different_identities() {
    let symbols = vec![
        symbol("a.rs", "parse", "function", 1, 3),
        symbol("b.rs", "parse", "function", 1, 3),
    ];
    let hashes = vec![
        ("a.rs".into(), "same".into()),
        ("b.rs".into(), "same".into()),
    ];
    let findings = dead_code_findings(&symbols, &[0, 1], &hashes);
    assert_ne!(findings[0].identity, findings[1].identity);
    assert_ne!(findings[0].legacy_key, findings[1].legacy_key);
}

#[test]
fn a_file_without_a_content_hash_has_unknown_content() {
    let symbols = vec![symbol("c.rs", "parse", "function", 1, 3)];
    let findings = dead_code_findings(&symbols, &[0], &hashes());
    assert!(!findings[0].content_known);
    assert!(findings[0].legacy_key.is_none());
}

#[test]
fn rapid_saves_do_not_push_the_clock_ahead_of_a_later_old_binary_decision() {
    let root = project();
    let mut reviews = Reviews::load(root.path());
    for round in 0..50 {
        let state = [State::InReview, State::Confirmed][round % 2];
        let revision = reviews.resolve(&[dup()])[0].revision();
        reviews
            .save(&[dup()], 0, &change(state, &revision), NOW)
            .unwrap();
    }
    // Logical time moved 49 ms past the clock, not 49 seconds.
    let latest = reviews.resolve(&[dup()]).remove(0).winner.unwrap().at;
    assert_eq!(latest, NOW + 49);
    // An old binary decides one second later: the later decision wins.
    write(
        root.path(),
        "duplicate-reviews.toon",
        &legacy_v2(&[('a', "dismissed", NOW_SECS + 1, "")], &[]),
    );
    assert_eq!(resolve_one(root.path(), &dup()).state(), State::Dismissed);
}
