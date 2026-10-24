//! Reads and rewrites the project's `.mctignore` for the settings page.
//!
//! Only that one file at the project root is ever touched; the request never
//! names a path. A write is refused when the file changed on disk since the
//! page loaded it, and goes through a temporary file plus rename so a failed
//! write cannot leave a truncated rule file behind.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;

use mct_index::{GITIGNORE_IMPORT_DIRECTIVE, IGNORE_FILE_NAME, IGNORE_FILE_TEMPLATE};
use serde_json::{json, Value};

use super::response::Response;
use super::Site;

/// Largest `.mctignore` the page reads or writes.
pub(super) const MAX_IGNORE_BYTES: usize = 64 * 1024;

const GITIGNORE_FILE_NAME: &str = ".gitignore";
const IMPORT_COMMAND: &str = "mct-cli ignore-init --import-gitignore";

/// Current file text, `None` when there is no file yet.
fn read(root: &Path) -> Result<Option<String>, String> {
    let path = root.join(IGNORE_FILE_NAME);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot read {IGNORE_FILE_NAME}: {error}")),
        Ok(metadata) if !metadata.is_file() => {
            return Err(format!("{IGNORE_FILE_NAME} must be a regular file."))
        }
        Ok(metadata) if metadata.len() > MAX_IGNORE_BYTES as u64 => {
            return Err(format!(
                "{IGNORE_FILE_NAME} is larger than {MAX_IGNORE_BYTES} bytes."
            ))
        }
        Ok(_) => {}
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("Cannot read {IGNORE_FILE_NAME}: {error}"))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{IGNORE_FILE_NAME} is not valid UTF-8."))
}

fn state_json(root: &Path) -> Result<Value, String> {
    let current = read(root)?;
    let content = current.clone().unwrap_or_default();
    Ok(json!({
        "path": IGNORE_FILE_NAME,
        "exists": current.is_some(),
        "content": content,
        "template": IGNORE_FILE_TEMPLATE,
        "directive": GITIGNORE_IMPORT_DIRECTIVE,
        "command": IMPORT_COMMAND,
        "importsGitignore": content.lines().any(|line| line.trim() == GITIGNORE_IMPORT_DIRECTIVE),
        "gitignorePresent": root.join(GITIGNORE_FILE_NAME).is_file(),
        "maxBytes": MAX_IGNORE_BYTES,
    }))
}

/// Writes `content` to `.mctignore` through a temporary file in the same directory.
fn write_atomically(root: &Path, content: &str) -> std::io::Result<()> {
    let target = root.join(IGNORE_FILE_NAME);
    let temporary = root.join(format!("{IGNORE_FILE_NAME}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        if let Ok(existing) = fs::metadata(&target) {
            let _ = fs::set_permissions(&temporary, existing.permissions());
        }
        fs::rename(&temporary, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

impl Site {
    pub(super) fn ignore_state(&self) -> Result<Value, (u16, &'static str)> {
        state_json(&self.root).map_err(|message| {
            eprintln!("warning: {message}");
            (500, "Cannot read .mctignore. See the server log.")
        })
    }

    /// Replaces `.mctignore` with the JSON body's `content` (`csrf`, `content`,
    /// `base`). `base` is the text the page loaded; if the file differs now,
    /// nothing is written and 409 returns the current state.
    pub(super) fn ignore(&mut self, body: &str) -> Response {
        let Ok(Value::Object(fields)) = serde_json::from_str(body) else {
            return Response::failure(400, "invalid JSON body");
        };
        if fields.get("csrf").and_then(Value::as_str) != Some(self.csrf.as_str()) {
            return Response::error(403);
        }
        if fields
            .keys()
            .any(|key| !["csrf", "content", "base"].contains(&key.as_str()))
        {
            return Response::failure(400, "unknown field");
        }
        let (Some(content), Some(base)) = (
            fields.get("content").and_then(Value::as_str),
            fields.get("base").and_then(Value::as_str),
        ) else {
            return Response::failure(400, "content and base must be strings");
        };
        if content.len() > MAX_IGNORE_BYTES {
            return Response::failure(413, "The rules are too large to save.");
        }
        if content.contains('\0') {
            return Response::failure(400, "The rules contain a NUL character.");
        }
        let current = match read(&self.root) {
            Ok(current) => current.unwrap_or_default(),
            Err(message) => {
                eprintln!("warning: {message}");
                return Response::failure(500, "Cannot read .mctignore. See the server log.");
            }
        };
        if current != base {
            return match state_json(&self.root) {
                Ok(mut state) => {
                    state["error"] = ".mctignore changed on disk meanwhile. Nothing was saved; \
                                      its current content is shown."
                        .into();
                    Response::json(&state).with_status(409)
                }
                Err(_) => Response::failure(500, "Cannot read .mctignore. See the server log."),
            };
        }
        let mut text = content.replace("\r\n", "\n");
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        if let Err(error) = write_atomically(&self.root, &text) {
            eprintln!("warning: cannot save {IGNORE_FILE_NAME}: {error}");
            return Response::failure(500, "Could not save .mctignore. No changes were applied.");
        }
        match state_json(&self.root) {
            Ok(state) => Response::json(&state),
            Err(_) => Response::failure(500, "Saved, but cannot read .mctignore back."),
        }
    }
}
