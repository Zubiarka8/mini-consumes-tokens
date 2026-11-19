//! A local history of incidents: one small file per occurrence in
//! `<root>/.mct-index/errors/`.
//!
//! This is a log of what happened, not the current state: the index keeps the
//! current file problems in `index_issues`, and the versioned
//! `errors/REGISTRY.md` is the manual record. The log is best effort. A caller
//! reports an incident next to the code that knows its cause, keeps the
//! operation's own result, and drops the `Err` this module returns; nothing
//! here logs its own failure.
//!
//! Rules that keep the log safe to call from anywhere:
//!
//! - It writes only when `<root>/.mct-index` already exists as a real
//!   directory, so a failure before the first `init` keeps its usual
//!   diagnostic and no directory appears as a side effect.
//! - It never follows a symlink for `.mct-index` or `errors`.
//! - A record is written in full to an exclusive temporary file and then
//!   published with `hard_link`, which fails instead of replacing an existing
//!   record. The temporary file is removed on every path.
//! - The name and the content carry only selected, sanitized fields. Callers
//!   never pass arguments, request bodies or notes.
//!
//! The `errors` directory sits under `.mct-index`, which the built-in exclusion
//! list already keeps out of the index and out of the watcher's events.

use std::collections::hash_map::RandomState;
use std::fs::{self, OpenOptions};
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const INDEX_DIR: &str = ".mct-index";
const ERRORS_DIR: &str = "errors";
/// Prefix of the exclusive temporary file; never ends in `.log`.
const TEMP_PREFIX: &str = ".tmp-";
/// Longest `operation` or `kind` kept in a file name.
const MAX_NAME_PART: usize = 40;
/// Longest diagnostic or path kept in a record, in bytes.
const MAX_FIELD_BYTES: usize = 2048;
/// Fresh ids tried before giving up on a name collision.
const MAX_ATTEMPTS: usize = 8;
const REDACTED: &str = "<redacted>";

/// One occurrence to record. Every field is chosen by the caller: short,
/// already-formed diagnostics, never arguments, bodies or notes.
#[derive(Debug, Clone, Copy)]
pub struct Incident<'a> {
    /// The operation that hit the problem, such as `reindex` or `batch`.
    pub operation: &'a str,
    /// A stable kind, such as `syntax-error` or `review-conflict`.
    pub kind: &'a str,
    /// The affected path, relative to the project root, when there is one.
    pub path: Option<&'a str>,
    /// The affected 1-based line, when the source of the incident knows it.
    pub line: Option<u32>,
    /// What went wrong.
    pub detail: &'a str,
}

/// Records `incident` under `root`.
///
/// Returns the published file, or `Ok(None)` when `<root>/.mct-index` does not
/// exist as a real directory (nothing is written then). An `Err` means the
/// record was not written; the caller keeps its own result and ignores it.
pub fn log_incident(root: &Path, incident: &Incident<'_>) -> io::Result<Option<PathBuf>> {
    let mut ids = IdSource::new();
    write_incident(root, incident, SystemTime::now(), &mut || ids.next())
}

/// [`log_incident`] with the clock and the id source injected, so tests can
/// force a collision.
pub(crate) fn write_incident(
    root: &Path,
    incident: &Incident<'_>,
    now: SystemTime,
    next_id: &mut dyn FnMut() -> String,
) -> io::Result<Option<PathBuf>> {
    let Some(dir) = errors_dir(root)? else {
        return Ok(None);
    };
    let stamp = utc_stamp(now);
    let content = render(root, incident, &stamp);
    let prefix = format!(
        "{stamp}--{}--{}--",
        name_part(incident.operation),
        name_part(incident.kind)
    );
    let temp = Temporary::create(&dir, next_id)?;
    let mut file = &temp.file;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    for _ in 0..MAX_ATTEMPTS {
        let target = dir.join(format!("{prefix}{}.log", next_id()));
        match fs::hard_link(&temp.path, &target) {
            Ok(()) => return Ok(Some(target)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free incident file name",
    ))
}

/// `<root>/.mct-index/errors`, created on demand. `None` when `.mct-index` is
/// missing or is not a real directory. `Err` when `errors` exists but is not a
/// real directory (a regular file or a symlink), which is never followed.
fn errors_dir(root: &Path) -> io::Result<Option<PathBuf>> {
    let index = root.join(INDEX_DIR);
    match fs::symlink_metadata(&index) {
        Ok(meta) if meta.is_dir() => {}
        _ => return Ok(None),
    }
    let errors = index.join(ERRORS_DIR);
    match fs::symlink_metadata(&errors) {
        Ok(meta) if meta.is_dir() => Ok(Some(errors)),
        Ok(_) => Err(not_a_directory()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match fs::create_dir(&errors) {
                Ok(()) => {}
                // Another writer created it first: it still has to be real.
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            match fs::symlink_metadata(&errors) {
                Ok(meta) if meta.is_dir() => Ok(Some(errors)),
                Ok(_) => Err(not_a_directory()),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

fn not_a_directory() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "the incident directory is not a real directory",
    )
}

/// An exclusive temporary file inside the incident directory, removed on drop
/// whether or not the record was published (the published name is a second
/// hard link to the same data).
struct Temporary {
    path: PathBuf,
    file: fs::File,
}

impl Temporary {
    fn create(dir: &Path, next_id: &mut dyn FnMut() -> String) -> io::Result<Self> {
        for _ in 0..MAX_ATTEMPTS {
            let path = dir.join(format!("{TEMP_PREFIX}{}", next_id()));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no free temporary file name",
        ))
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Ids for the file names: 12 hex digits from a per-process random hasher
/// mixed with the pid, the clock and a counter, so concurrent threads and
/// processes rarely collide. A collision is retried anyway.
struct IdSource {
    state: RandomState,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

impl IdSource {
    fn new() -> Self {
        Self {
            state: RandomState::new(),
        }
    }

    fn next(&mut self) -> String {
        let mut hasher = self.state.build_hasher();
        hasher.write_u32(std::process::id());
        hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        hasher.write_u128(nanos);
        format!("{:012x}", hasher.finish() & 0xffff_ffff_ffff)
    }
}

fn render(root: &Path, incident: &Incident<'_>, stamp: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("date: {}\n", iso_from_stamp(stamp)));
    out.push_str(&format!("operation: {}\n", name_part(incident.operation)));
    out.push_str(&format!("kind: {}\n", name_part(incident.kind)));
    if let Some(path) = incident.path {
        out.push_str(&format!("path: {}\n", clean_text(root, path)));
    }
    if let Some(line) = incident.line {
        out.push_str(&format!("line: {line}\n"));
    }
    out.push_str(&format!("detail: {}\n", clean_text(root, incident.detail)));
    out
}

/// A file-name part: lowercase ASCII letters and digits joined by single
/// dashes, at most [`MAX_NAME_PART`] bytes, `unknown` when nothing is left.
fn name_part(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.truncate(MAX_NAME_PART);
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

/// Text for the record: no control characters, the project root and the home
/// directory replaced, known secret shapes redacted, and a length cap.
fn clean_text(root: &Path, raw: &str) -> String {
    let mut text: String = raw
        .chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();
    let root_text = root.display().to_string();
    if root_text.len() > 1 {
        text = text.replace(&root_text, "<root>");
    }
    for var in ["HOME", "USERPROFILE"] {
        if let Some(home) = std::env::var_os(var) {
            let home = home.to_string_lossy();
            if home.len() > 1 {
                text = text.replace(home.as_ref(), "~");
            }
        }
    }
    let mut text = redact_secrets(&text);
    if text.len() > MAX_FIELD_BYTES {
        let mut end = MAX_FIELD_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("...");
    }
    text
}

/// Keywords whose value follows an `=` or `:`; the value is replaced.
const SECRET_KEYS: [&str; 7] = [
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "api-key",
    "apikey",
];

/// A second layer, not the guarantee: callers already pass only diagnostics.
/// Redacts the value after `key=` or `key:` for the keys above, the rest of
/// the value after `authorization:`, and the word after `bearer`.
fn redact_secrets(text: &str) -> String {
    // ASCII lowercasing keeps byte offsets, so indexes match `text`.
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    while pos < text.len() {
        let rest = &lower[pos..];
        let hit = SECRET_KEYS
            .iter()
            .map(|key| (*key, ValueEnds::Word))
            .chain([
                ("authorization", ValueEnds::Quote),
                ("bearer", ValueEnds::BearerWord),
            ])
            .filter_map(|(key, ends)| rest.starts_with(key).then_some((key, ends)))
            .next();
        let Some((key, ends)) = hit else {
            let Some(c) = text[pos..].chars().next() else {
                break;
            };
            out.push(c);
            pos += c.len_utf8();
            continue;
        };
        let after_key = pos + key.len();
        let value_start = match ends {
            ValueEnds::BearerWord => skip(text, after_key, |c| c == ' '),
            _ => {
                let at = skip(text, after_key, |c| c == ' ' || c == '"' || c == '\'');
                if !text[at..].starts_with(['=', ':']) {
                    out.push_str(&text[pos..after_key]);
                    pos = after_key;
                    continue;
                }
                skip(text, at + 1, |c| c == ' ' || c == '"' || c == '\'')
            }
        };
        let value_end = value_start
            + text[value_start..]
                .find(|c: char| match ends {
                    ValueEnds::Quote => matches!(c, '"' | '\'' | ',' | ';'),
                    _ => c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';'),
                })
                .unwrap_or(text.len() - value_start);
        if value_end == value_start {
            out.push_str(&text[pos..after_key]);
            pos = after_key;
            continue;
        }
        out.push_str(&text[pos..value_start]);
        out.push_str(REDACTED);
        pos = value_end;
    }
    out
}

#[derive(Clone, Copy)]
enum ValueEnds {
    Word,
    Quote,
    BearerWord,
}

fn skip(text: &str, from: usize, skippable: impl Fn(char) -> bool) -> usize {
    from + text[from..]
        .find(|c: char| !skippable(c))
        .unwrap_or(text.len() - from)
}

/// `YYYYMMDDTHHMMSS.ffffffZ` in UTC.
fn utc_stamp(now: SystemTime) -> String {
    let since = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs();
    let micros = since.subsec_micros();
    let (year, month, day) = civil_from_days(secs / 86_400);
    let rem = secs % 86_400;
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}.{micros:06}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// `2026-11-19T15:30:45.123456Z` from [`utc_stamp`]'s compact form.
fn iso_from_stamp(stamp: &str) -> String {
    match (
        stamp.get(0..4),
        stamp.get(4..6),
        stamp.get(6..8),
        stamp.get(9..11),
        stamp.get(11..13),
        stamp.get(13..),
    ) {
        (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(rest)) => {
            format!("{y}-{mo}-{d}T{h}:{mi}:{rest}")
        }
        _ => stamp.to_string(),
    }
}

/// Days since 1970-01-01 to a proleptic Gregorian (year, month, day).
/// Howard Hinnant's `civil_from_days`, for non-negative days.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use std::time::Duration;

    fn at(secs: u64, micros: u32) -> SystemTime {
        UNIX_EPOCH + Duration::new(secs, micros * 1000)
    }

    #[test]
    fn utc_stamp_matches_known_instants() {
        assert_eq!(utc_stamp(at(0, 0)), "19700101T000000.000000Z");
        // 2026-11-19T15:30:45.123456Z
        assert_eq!(utc_stamp(at(1_795_102_245, 123_456)), "20261119T153045.123456Z");
        // Leap day: 2024-02-29T23:59:59Z
        assert_eq!(utc_stamp(at(1_709_251_199, 0)), "20240229T235959.000000Z");
        // 2000-03-01T00:00:00Z, after the 400-year leap rule.
        assert_eq!(utc_stamp(at(951_868_800, 5)), "20000301T000000.000005Z");
    }

    #[test]
    fn iso_form_keeps_every_digit() {
        assert_eq!(
            iso_from_stamp("20261119T153045.123456Z"),
            "2026-11-19T15:30:45.123456Z"
        );
    }

    #[test]
    fn name_parts_are_safe() {
        assert_eq!(name_part("Syntax Error"), "syntax-error");
        assert_eq!(name_part("../../etc/passwd"), "etc-passwd");
        assert_eq!(name_part("a--b"), "a-b");
        assert_eq!(name_part("\u{0}\n"), "unknown");
        assert_eq!(name_part(&"x".repeat(200)).len(), MAX_NAME_PART);
    }

    #[test]
    fn secrets_are_redacted() {
        assert_eq!(redact_secrets("password=hunter2 ok"), "password=<redacted> ok");
        assert_eq!(
            redact_secrets("Authorization: Bearer abc.def"),
            "Authorization: <redacted>"
        );
        assert_eq!(redact_secrets("sent bearer abc now"), "sent bearer <redacted> now");
        assert_eq!(redact_secrets(r#"{"token": "t0k3n"}"#), r#"{"token": "<redacted>"}"#);
        // A parser message that merely mentions the word stays readable.
        assert_eq!(
            redact_secrets("line 3: unexpected token `}`"),
            "line 3: unexpected token `}`"
        );
    }

    #[test]
    fn long_text_is_cut_on_a_char_boundary() {
        let text = clean_text(Path::new("/p"), &"é".repeat(MAX_FIELD_BYTES));
        assert!(text.ends_with("..."));
        assert!(text.len() <= MAX_FIELD_BYTES + 3);
    }

    #[test]
    fn a_forced_collision_retries_and_exhaustion_fails() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(INDEX_DIR)).unwrap();
        let incident = Incident {
            operation: "reindex",
            kind: "syntax-error",
            path: None,
            line: None,
            detail: "boom",
        };
        let now = at(1_795_102_245, 123_456);
        let name = |id: &str| format!("20261119T153045.123456Z--reindex--syntax-error--{id}.log");

        // The temporary takes "t0"; "aa" is taken, "bb" is free.
        let errors = dir.path().join(INDEX_DIR).join(ERRORS_DIR);
        std::fs::create_dir(&errors).unwrap();
        std::fs::write(errors.join(name("aa")), "older").unwrap();
        let mut ids = ["t0", "aa", "bb"].into_iter().map(String::from);
        let published = write_incident(dir.path(), &incident, now, &mut || ids.next().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(published, errors.join(name("bb")));
        assert_eq!(std::fs::read_to_string(errors.join(name("aa"))).unwrap(), "older");
        assert!(!errors.join(".tmp-t0").exists());

        // Every candidate name taken: an error, the older record untouched.
        let mut calls = 0;
        let result = write_incident(dir.path(), &incident, now, &mut || {
            calls += 1;
            if calls == 1 { "t1".into() } else { "aa".into() }
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert!(!errors.join(".tmp-t1").exists());
        assert_eq!(std::fs::read_to_string(errors.join(name("aa"))).unwrap(), "older");
    }

    #[test]
    fn a_taken_temporary_name_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let errors = dir.path().join(INDEX_DIR).join(ERRORS_DIR);
        std::fs::create_dir_all(&errors).unwrap();
        std::fs::write(errors.join(".tmp-t0"), "other writer").unwrap();
        let incident = Incident {
            operation: "op",
            kind: "k",
            path: None,
            line: None,
            detail: "d",
        };
        let mut ids = ["t0", "t1", "zz"].into_iter().map(String::from);
        let published = write_incident(dir.path(), &incident, at(0, 0), &mut || {
            ids.next().unwrap()
        })
        .unwrap()
        .unwrap();
        assert!(published.ends_with("19700101T000000.000000Z--op--k--zz.log"));
        // The other writer's temporary is not ours to remove.
        assert!(errors.join(".tmp-t0").exists());
    }
}
