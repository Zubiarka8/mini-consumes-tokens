//! Parsers for every review file format, and the writer for `C3`.
//!
//! The legacy parsers accept exactly what their original readers accepted
//! (`crates/mct-cli/src/web/reviews.rs` at `e68aec9` for `A1`/`A2`, and the
//! dashboard draft for `B2`), so a file an old binary reads is never reported
//! as corrupt here, and a file it rejects is never trusted here.

use std::collections::BTreeMap;
use std::fmt;

use super::{Kind, State};

pub(super) const MAX_ROWS: usize = 100_000;
pub(super) const MAX_NOTE_CHARS: usize = 500;
/// The reason given for a `C3` file written by a newer version.
pub(super) const NEWER_FORMAT: &str = "written by a newer version of mini-consumes-tokens";
const C3_REVIEWS: &str = "{kind,identity,fingerprint,state,updated,note}:";
const C3_HISTORY: &str = "{kind,identity,fingerprint,from,to,at,note}:";
const B2_HEADER: &str = "{kind,identity,fingerprint,status,at,reason}:";
const B2_MAX_HISTORY: usize = 50_000;
const B2_MAX_REASON: usize = 500;

/// Why a file cannot be read, with its 1-based line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FormatError {
    pub line: usize,
    pub reason: &'static str,
}

impl fmt::Display for FormatError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "line {}: {}", self.line, self.reason)
    }
}

type Parsed<T> = Result<T, FormatError>;

fn fail<T>(line: usize, reason: &'static str) -> Parsed<T> {
    Err(FormatError { line, reason })
}

fn check(condition: bool, line: usize, reason: &'static str) -> Parsed<()> {
    if condition {
        Ok(())
    } else {
        fail(line, reason)
    }
}

pub(super) fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub(super) fn valid_note(note: &str) -> bool {
    note.chars().count() <= MAX_NOTE_CHARS && !note.chars().any(char::is_control)
}

fn encode(note: &str) -> String {
    form_urlencoded::byte_serialize(note.as_bytes()).collect()
}

fn decode(note: &str) -> String {
    if note.is_empty() {
        return String::new();
    }
    form_urlencoded::parse(format!("n={note}").as_bytes())
        .next()
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}

/// The lines of a file, numbered from 1.
struct Lines<'a> {
    lines: std::iter::Enumerate<std::str::Lines<'a>>,
    last: usize,
}

impl<'a> Lines<'a> {
    fn new(content: &'a str) -> Self {
        Self {
            lines: content.lines().enumerate(),
            last: 0,
        }
    }

    fn next(&mut self) -> Option<(usize, &'a str)> {
        let (index, line) = self.lines.next()?;
        self.last = index + 1;
        Some((self.last, line))
    }

    /// The next line, or an error on the line after the last one.
    fn require(&mut self, reason: &'static str) -> Parsed<(usize, &'a str)> {
        let after = self.last + 1;
        self.next().ok_or(FormatError {
            line: after,
            reason,
        })
    }
}

/// `name[count]{columns}` header; returns the count.
fn header(line: (usize, &str), name: &str, columns: &str, limit: usize) -> Parsed<usize> {
    let (number, text) = line;
    let count = text
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('['))
        .and_then(|rest| rest.strip_suffix(&format!("]{columns}")))
        .and_then(|count| count.parse::<usize>().ok());
    match count {
        Some(count) if count <= limit => Ok(count),
        Some(_) => fail(number, "too many rows"),
        None => fail(number, "invalid table header"),
    }
}

fn row(line: (usize, &str)) -> Parsed<(usize, &str)> {
    match line.1.strip_prefix("  ") {
        Some(text) => Ok((line.0, text)),
        None => fail(line.0, "invalid row"),
    }
}

fn time(number: usize, text: &str) -> Parsed<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return fail(number, "invalid time");
    }
    text.parse().or(fail(number, "invalid time"))
}

// ---------- C3: review-decisions.toon, version 3 ----------

/// One current decision. Its identity is the key in [`Canonical::reviews`],
/// stored once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Row {
    pub kind: Kind,
    pub fingerprint: String,
    pub state: State,
    /// Unix milliseconds.
    pub updated: u64,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Event {
    pub kind: Kind,
    pub identity: String,
    pub fingerprint: String,
    pub from: State,
    pub to: State,
    /// Unix milliseconds.
    pub at: u64,
    pub note: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Canonical {
    /// Keyed by `(kind, identity)`, which is unique.
    pub reviews: BTreeMap<(Kind, String), Row>,
    /// Append order.
    pub history: Vec<Event>,
}

/// Splits a `C3` row into exactly `N` fields; the last one is a note, which
/// is URL-encoded and so never contains a comma.
fn fields<const N: usize>(number: usize, text: &str) -> Parsed<[&str; N]> {
    let mut parts = text.split(',');
    let mut out = [""; N];
    for slot in &mut out {
        *slot = match parts.next() {
            Some(part) => part,
            None => return fail(number, "wrong number of fields"),
        };
    }
    match parts.next() {
        Some(_) => fail(number, "wrong number of fields"),
        None => Ok(out),
    }
}

fn keys<'a>(number: usize, kind: &str, identity: &'a str, fingerprint: &'a str) -> Parsed<Kind> {
    let kind = Kind::parse(kind).ok_or(FormatError {
        line: number,
        reason: "unknown kind",
    })?;
    check(
        valid_key(identity) && valid_key(fingerprint),
        number,
        "invalid key",
    )?;
    Ok(kind)
}

fn state(number: usize, text: &str) -> Parsed<State> {
    State::parse(text).ok_or(FormatError {
        line: number,
        reason: "unknown state",
    })
}

fn note(number: usize, encoded: &str) -> Parsed<String> {
    if encoded.is_empty() {
        return Ok(String::new());
    }
    let note = decode(encoded);
    // Canonical encoding only, so a file always renders back byte for byte.
    check(
        encode(&note) == encoded && valid_note(&note),
        number,
        "invalid note",
    )?;
    Ok(note)
}

pub(super) fn parse_canonical(content: &str) -> Parsed<Canonical> {
    let mut lines = Lines::new(content);
    match lines.next() {
        Some((_, "version: 3")) => (),
        Some((number, text)) => {
            let newer = text
                .strip_prefix("version: ")
                .and_then(|version| version.parse::<u64>().ok())
                .is_some_and(|version| version > 3);
            return fail(
                number,
                if newer {
                    NEWER_FORMAT
                } else {
                    "unsupported format"
                },
            );
        }
        None => return fail(1, "empty file"),
    }
    let count = header(
        lines.require("missing reviews table")?,
        "reviews",
        C3_REVIEWS,
        MAX_ROWS,
    )?;
    let mut reviews = BTreeMap::new();
    for _ in 0..count {
        let (number, text) = row(lines.require("missing review row")?)?;
        let [kind, identity, fingerprint, state_text, updated, encoded] =
            fields::<6>(number, text)?;
        let kind = keys(number, kind, identity, fingerprint)?;
        let row = Row {
            kind,
            fingerprint: fingerprint.to_string(),
            state: state(number, state_text)?,
            updated: time(number, updated)?,
            note: note(number, encoded)?,
        };
        check(
            reviews.insert((kind, identity.to_string()), row).is_none(),
            number,
            "duplicate review",
        )?;
    }
    let count = header(
        lines.require("missing history table")?,
        "history",
        C3_HISTORY,
        MAX_ROWS,
    )?;
    let mut history = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let (number, text) = row(lines.require("missing history row")?)?;
        let [kind, identity, fingerprint, from, to, at, encoded] = fields::<7>(number, text)?;
        history.push(Event {
            kind: keys(number, kind, identity, fingerprint)?,
            identity: identity.to_string(),
            fingerprint: fingerprint.to_string(),
            from: state(number, from)?,
            to: state(number, to)?,
            at: time(number, at)?,
            note: note(number, encoded)?,
        });
    }
    if let Some((number, _)) = lines.next() {
        return fail(number, "unexpected data after the history table");
    }
    Ok(Canonical { reviews, history })
}

pub(super) fn render_canonical(store: &Canonical) -> String {
    let mut out = format!("version: 3\nreviews[{}]{C3_REVIEWS}\n", store.reviews.len());
    for ((_, identity), row) in &store.reviews {
        out.push_str(&format!(
            "  {},{},{},{},{},{}\n",
            row.kind.value(),
            identity,
            row.fingerprint,
            row.state.value(),
            row.updated,
            encode(&row.note)
        ));
    }
    out.push_str(&format!("history[{}]{C3_HISTORY}\n", store.history.len()));
    for event in &store.history {
        out.push_str(&format!(
            "  {},{},{},{},{},{},{}\n",
            event.kind.value(),
            event.identity,
            event.fingerprint,
            event.from.value(),
            event.to.value(),
            event.at,
            encode(&event.note)
        ));
    }
    out
}

// ---------- B2: reviews.toon, version 2 (dashboard draft) ----------

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DraftRow {
    pub kind: Kind,
    pub identity: String,
    pub fingerprint: String,
    /// `pending` maps to `Unreviewed`: a restore tombstone.
    pub state: State,
    /// Unix seconds.
    pub at: u64,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Draft {
    pub reviews: BTreeMap<(Kind, String), DraftRow>,
    pub history: Vec<DraftRow>,
}

/// The draft's reason cleaning: rows whose reason it would change are invalid.
fn clean_reason(raw: &str) -> String {
    let text: String = raw
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let mut text = text.trim().to_string();
    if text.len() > B2_MAX_REASON {
        let mut end = B2_MAX_REASON;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text = text.trim_end().to_string();
    }
    text
}

fn draft_table(lines: &mut Lines<'_>, name: &str, limit: usize) -> Parsed<Vec<DraftRow>> {
    let count = header(lines.require("missing table")?, name, B2_HEADER, limit)?;
    let mut rows = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let (number, text) = row(lines.require("missing row")?)?;
        let mut parts = text.splitn(6, ',');
        let mut next = || {
            parts.next().ok_or(FormatError {
                line: number,
                reason: "short row",
            })
        };
        let kind = match next()? {
            "duplicate" => Kind::Duplicate,
            "dead" => Kind::DeadCode,
            _ => return fail(number, "unknown kind"),
        };
        let identity = next()?;
        let fingerprint = next()?;
        let state = match next()? {
            "pending" => State::Unreviewed,
            "confirmed" => State::Confirmed,
            "false_positive" => State::FalsePositive,
            "ignored" => State::Dismissed,
            _ => return fail(number, "unknown status"),
        };
        let at: u64 = next()?.parse().or(fail(number, "invalid time"))?;
        let reason = next()?;
        check(
            valid_key(identity) && valid_key(fingerprint),
            number,
            "invalid key",
        )?;
        check(reason == clean_reason(reason), number, "invalid reason")?;
        rows.push(DraftRow {
            kind,
            identity: identity.to_string(),
            fingerprint: fingerprint.to_string(),
            state,
            at,
            reason: reason.to_string(),
        });
    }
    Ok(rows)
}

pub(super) fn parse_draft(content: &str) -> Parsed<Draft> {
    let mut lines = Lines::new(content);
    check(
        lines.next().map(|(_, text)| text) == Some("version: 2"),
        1,
        "unsupported format",
    )?;
    let current = draft_table(&mut lines, "reviews", MAX_ROWS)?;
    let history = draft_table(&mut lines, "history", B2_MAX_HISTORY)?;
    if let Some((number, _)) = lines.next() {
        return fail(number, "unexpected data after the history table");
    }
    let mut reviews = BTreeMap::new();
    for row in current {
        if reviews
            .insert((row.kind, row.identity.clone()), row)
            .is_some()
        {
            return fail(2, "duplicate review");
        }
    }
    Ok(Draft { reviews, history })
}

// ---------- A1/A2: duplicate-reviews.toon, versions 1 and 2 ----------

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LegacyEntry {
    pub state: State,
    /// Unix seconds; 0 for version 1 rows, which have no time.
    pub updated: u64,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LegacyEvent {
    pub key: String,
    /// `none` maps to `Unreviewed`.
    pub from: State,
    pub to: State,
    /// Unix seconds.
    pub at: u64,
    pub note: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Legacy {
    pub version: u8,
    pub entries: BTreeMap<String, LegacyEntry>,
    /// Append order (version 2 only).
    pub history: Vec<LegacyEvent>,
}

/// Version 1 reasons and version 2 states, as the `A2` reader parses them.
fn legacy_state(number: usize, text: &str) -> Parsed<State> {
    match State::parse(text) {
        Some(State::Unreviewed) | None => fail(number, "invalid review state"),
        Some(state) => Ok(state),
    }
}

fn legacy_optional_state(number: usize, text: &str) -> Parsed<State> {
    if text == "none" {
        Ok(State::Unreviewed)
    } else {
        legacy_state(number, text)
    }
}

pub(super) fn parse_legacy(content: &str) -> Parsed<Legacy> {
    let mut lines = Lines::new(content);
    match lines.next().map(|(_, text)| text) {
        Some("version: 1") => parse_legacy_v1(&mut lines),
        Some("version: 2") => parse_legacy_v2(&mut lines),
        _ => fail(1, "unsupported format"),
    }
}

fn parse_legacy_v1(lines: &mut Lines<'_>) -> Parsed<Legacy> {
    let count = header(
        lines.require("missing reviews table")?,
        "reviews",
        "{key,reason}:",
        MAX_ROWS,
    )?;
    let mut entries = BTreeMap::new();
    while let Some(line) = lines.next() {
        let (number, text) = row(line)?;
        let Some((key, reason)) = text.split_once(',') else {
            return fail(number, "invalid row");
        };
        check(valid_key(key), number, "invalid key")?;
        let entry = LegacyEntry {
            state: legacy_state(number, reason)?,
            updated: 0,
            note: String::new(),
        };
        check(
            entries.insert(key.to_string(), entry).is_none(),
            number,
            "duplicate review",
        )?;
    }
    check(entries.len() == count, lines.last, "review count mismatch")?;
    Ok(Legacy {
        version: 1,
        entries,
        history: Vec::new(),
    })
}

fn parse_legacy_v2(lines: &mut Lines<'_>) -> Parsed<Legacy> {
    let count = header(
        lines.require("missing reviews table")?,
        "reviews",
        "{key,state,updated,note}:",
        MAX_ROWS,
    )?;
    let mut entries = BTreeMap::new();
    for _ in 0..count {
        let (number, text) = row(lines.require("missing review row")?)?;
        let mut parts = text.splitn(4, ',');
        let (Some(key), Some(state), Some(updated), Some(note)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return fail(number, "invalid row");
        };
        check(valid_key(key), number, "invalid key")?;
        let entry = LegacyEntry {
            state: legacy_state(number, state)?,
            updated: updated.parse().or(fail(number, "invalid time"))?,
            note: decode(note),
        };
        check(
            entries.insert(key.to_string(), entry).is_none(),
            number,
            "duplicate review",
        )?;
    }
    let count = header(
        lines.require("missing history table")?,
        "history",
        "{key,from,to,at,note}:",
        MAX_ROWS,
    )?;
    let mut history = Vec::new();
    while let Some(line) = lines.next() {
        let (number, text) = row(line)?;
        let mut parts = text.splitn(5, ',');
        let (Some(key), Some(from), Some(to), Some(at), Some(note)) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return fail(number, "invalid history row");
        };
        check(valid_key(key), number, "invalid key")?;
        history.push(LegacyEvent {
            key: key.to_string(),
            from: legacy_optional_state(number, from)?,
            to: legacy_optional_state(number, to)?,
            at: at.parse().or(fail(number, "invalid time"))?,
            note: decode(note),
        });
    }
    check(history.len() == count, lines.last, "history count mismatch")?;
    Ok(Legacy {
        version: 2,
        entries,
        history,
    })
}
