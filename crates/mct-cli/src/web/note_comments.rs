//! Personal comments on Obsidian notes, kept beside the index.
//!
//! The vault itself is regenerated from the index every time the dashboard
//! starts, so a comment is never written into a note: it lives in
//! `.mct-index/note-comments.json`, keyed by the note's vault path.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::ignore_file::write_atomically;
use super::response::Response;
use super::Site;

/// Largest comment the page saves for one note.
pub(super) const MAX_COMMENT_BYTES: usize = 8 * 1024;
const MAX_COMMENTS: usize = 5000;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const FILE: &str = ".mct-index/note-comments.json";

/// Saved comments by note path: `{ "text": …, "updated": … }`.
type Store = BTreeMap<String, Value>;

fn load(root: &Path) -> Result<Store, String> {
    let path = root.join(FILE);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Store::new()),
        Err(error) => return Err(format!("Cannot read {FILE}: {error}")),
        Ok(metadata) if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES => {
            return Err(format!(
                "{FILE} must be a regular file under {MAX_FILE_BYTES} bytes."
            ))
        }
        Ok(_) => {}
    }
    let bytes = fs::read(&path).map_err(|error| format!("Cannot read {FILE}: {error}"))?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("{FILE} is not valid: {error}"))?;
    match value.get("comments") {
        Some(Value::Object(map)) => Ok(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        _ => Err(format!("{FILE} has no comments object.")),
    }
}

/// The saved comment of `note`, or null.
pub(super) fn get(root: &Path, note: &str) -> Value {
    load(root)
        .ok()
        .and_then(|mut store| store.remove(note))
        .unwrap_or(Value::Null)
}

impl Site {
    /// Saves (or, with empty `text`, removes) the comment of one note. The body is
    /// `{ csrf, path, text }`; `path` must be a note of the generated vault.
    pub(super) fn comment(&mut self, body: &str) -> Response {
        let Ok(Value::Object(fields)) = serde_json::from_str(body) else {
            return Response::failure(400, "invalid JSON body");
        };
        if fields.get("csrf").and_then(Value::as_str) != Some(self.csrf.as_str()) {
            return Response::error(403);
        }
        if fields
            .keys()
            .any(|key| !["csrf", "path", "text"].contains(&key.as_str()))
        {
            return Response::failure(400, "unknown field");
        }
        let (Some(path), Some(text)) = (
            fields.get("path").and_then(Value::as_str),
            fields.get("text").and_then(Value::as_str),
        ) else {
            return Response::failure(400, "path and text must be strings");
        };
        if !self.vault.notes.iter().any(|note| note.path == path) {
            return Response::failure(404, "unknown note");
        }
        if text.len() > MAX_COMMENT_BYTES {
            return Response::failure(413, "The comment is too long to save.");
        }
        if text.contains('\0') {
            return Response::failure(400, "The comment contains a NUL character.");
        }
        let mut store = match load(&self.root) {
            Ok(store) => store,
            Err(message) => {
                eprintln!("warning: {message}");
                return Response::failure(
                    500,
                    "Cannot read the saved comments. See the server log.",
                );
            }
        };
        let text = text.replace("\r\n", "\n");
        let result = if text.trim().is_empty() {
            store.remove(path);
            Value::Null
        } else {
            if !store.contains_key(path) && store.len() >= MAX_COMMENTS {
                return Response::failure(413, "Too many commented notes.");
            }
            let updated = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs());
            let comment = json!({ "text": text, "updated": updated });
            store.insert(path.to_string(), comment.clone());
            comment
        };
        let saved = serde_json::to_string_pretty(&json!({ "version": 1, "comments": store }))
            .map_err(std::io::Error::other)
            .and_then(|json| write_atomically(&self.root.join(FILE), &format!("{json}\n")));
        if let Err(error) = saved {
            eprintln!("warning: cannot save {FILE}: {error}");
            return Response::failure(500, "Could not save the comment. No changes were applied.");
        }
        Response::json(&json!({ "path": path, "comment": result }))
    }
}
