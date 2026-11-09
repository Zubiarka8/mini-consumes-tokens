//! Dashboard preferences kept beside the index, in `.mct-index/web-settings.json`.
//!
//! The browser keeps its own copy (`localStorage` and a cookie), which does not
//! survive cleared site data or another browser. This file does, and it is what
//! a restarted server reads for how long it may stay up without use. Only known
//! fields with valid values are stored; anything else in the file is dropped.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::idle::{Activity, MAX_MINUTES};
use super::response::Response;

const FILE: &str = "web-settings.json";
/// Longest file read: the real one holds four short values.
const MAX_BYTES: u64 = 4096;

fn path(root: &Path) -> PathBuf {
    root.join(".mct-index").join(FILE)
}

fn valid(key: &str, value: &Value) -> bool {
    match key {
        "theme" => matches!(value.as_str(), Some("dark" | "light" | "system")),
        "lang" => matches!(value.as_str(), Some("es" | "en")),
        // Minutes without use before the server closes / before the page pauses its live checks.
        "stop" | "pause" => value.as_u64().is_some_and(|minutes| minutes <= MAX_MINUTES),
        _ => false,
    }
}

/// The saved preferences; a missing, oversized or unreadable file is no preferences.
pub(super) fn load(root: &Path) -> Map<String, Value> {
    let Ok(file) = fs::File::open(path(root)) else {
        return Map::new();
    };
    let mut text = String::new();
    if std::io::Read::read_to_string(&mut std::io::Read::take(file, MAX_BYTES), &mut text).is_err()
    {
        return Map::new();
    }
    let Ok(Value::Object(fields)) = serde_json::from_str(&text) else {
        return Map::new();
    };
    fields
        .into_iter()
        .filter(|(key, value)| valid(key, value))
        .collect()
}

/// The saved minutes the server may stay up without use, if any.
pub(super) fn saved_stop_minutes(root: &Path) -> Option<u64> {
    load(root).get("stop").and_then(Value::as_u64)
}

pub(super) fn state(root: &Path) -> Response {
    Response::json(&Value::Object(load(root)))
}

fn write_atomically(root: &Path, fields: &Map<String, Value>) -> std::io::Result<()> {
    let target = path(root);
    let temporary = target.with_extension(format!("json.{}.tmp", std::process::id()));
    let text = serde_json::to_string_pretty(&Value::Object(fields.clone()))
        .map_err(std::io::Error::other)?;
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Applies `{"csrf": …, "theme"?, "lang"?, "stop"?, "pause"?}`: the given fields replace the
/// saved ones, and a new `stop` also applies to the running server.
pub(super) fn save(root: &Path, body: &str, csrf: &str, activity: &Activity) -> Response {
    let Ok(Value::Object(mut fields)) = serde_json::from_str::<Value>(body) else {
        return Response::failure(400, "invalid JSON body");
    };
    if fields.remove("csrf").as_ref().and_then(Value::as_str) != Some(csrf) {
        return Response::error(403);
    }
    if fields.is_empty() || fields.iter().any(|(key, value)| !valid(key, value)) {
        return Response::failure(400, "unknown setting or invalid value");
    }
    let mut all = load(root);
    all.extend(fields.clone());
    if !root.join(".mct-index").is_dir() {
        return Response::failure(500, "The index folder is missing.");
    }
    if let Err(error) = write_atomically(root, &all) {
        eprintln!("warning: cannot save the dashboard settings: {error}");
        return Response::failure(500, "The settings could not be saved.");
    }
    if let Some(minutes) = fields.get("stop").and_then(Value::as_u64) {
        activity.set_minutes(minutes);
    }
    Response::json(&json!(all))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn project() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".mct-index")).unwrap();
        root
    }

    #[test]
    fn saved_values_are_read_back_merged_and_applied_to_the_running_server() {
        let root = project();
        let activity = Activity::new();
        assert!(load(root.path()).is_empty());
        let saved = save(
            root.path(),
            r#"{"csrf":"k","theme":"system","stop":10}"#,
            "k",
            &activity,
        );
        assert_eq!(saved.status, 200);
        assert_eq!(activity.minutes(), 10);
        save(
            root.path(),
            r#"{"csrf":"k","lang":"es","pause":0}"#,
            "k",
            &activity,
        );
        let all = load(root.path());
        assert_eq!(all["theme"], "system");
        assert_eq!(all["lang"], "es");
        assert_eq!(all["stop"], 10);
        assert_eq!(all["pause"], 0);
        assert_eq!(saved_stop_minutes(root.path()), Some(10));
    }

    #[test]
    fn nothing_is_saved_without_the_token_or_with_a_bad_value() {
        let root = project();
        let activity = Activity::new();
        for (body, status) in [
            (r#"{"csrf":"x","stop":5}"#, 403),
            (r#"{"csrf":"k","stop":1441}"#, 400),
            (r#"{"csrf":"k","stop":-1}"#, 400),
            (r#"{"csrf":"k","theme":"pink"}"#, 400),
            (r#"{"csrf":"k","lang":"fr"}"#, 400),
            (r#"{"csrf":"k","other":1}"#, 400),
            (r#"{"csrf":"k"}"#, 400),
            ("not json", 400),
        ] {
            assert_eq!(
                save(root.path(), body, "k", &activity).status,
                status,
                "{body}"
            );
        }
        assert!(!path(root.path()).exists());
        assert_eq!(activity.minutes(), 0);
    }

    #[test]
    fn a_damaged_or_foreign_file_is_no_preferences_and_unknown_fields_are_dropped() {
        let root = project();
        fs::write(path(root.path()), "{ broken").unwrap();
        assert!(load(root.path()).is_empty());
        fs::write(
            path(root.path()),
            r#"{"theme":"dark","stop":"x","extra":1}"#,
        )
        .unwrap();
        let all = load(root.path());
        assert_eq!(all.len(), 1);
        assert_eq!(all["theme"], "dark");
    }
}
