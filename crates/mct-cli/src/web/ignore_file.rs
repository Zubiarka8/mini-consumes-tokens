//! Reads and rewrites the project's `.mctignore` and `.gitignore` for the dashboard.
//!
//! Only those two files at the project root are ever touched, chosen from a
//! fixed list ([`Rules`]); the request never names a path. A write is refused
//! when the file changed on disk since the page loaded it, and goes through a
//! temporary file plus rename so a failed write cannot leave a truncated rule
//! file behind.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;

use mct_index::{GITIGNORE_IMPORT_DIRECTIVE, IGNORE_FILE_NAME, IGNORE_FILE_TEMPLATE};
use serde_json::{json, Value};

use super::api::{text, Params};
use super::response::Response;
use super::Site;

/// Largest rule file the page reads or writes.
pub(super) const MAX_IGNORE_BYTES: usize = 64 * 1024;

const GITIGNORE_FILE_NAME: &str = ".gitignore";
const IMPORT_COMMAND: &str = "mct-cli ignore-init --import-gitignore";

/// The rule files the dashboard can edit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rules {
    /// `.mctignore`: what the index leaves out, on top of the built-in exclusions.
    Mct,
    /// `.gitignore`: what git leaves out; the index follows it through `@import-gitignore`.
    Git,
}

impl Rules {
    /// The request's `file` value: absent or empty means `.mctignore`.
    fn named(name: &str) -> Option<Self> {
        match name {
            "" | "mctignore" => Some(Self::Mct),
            "gitignore" => Some(Self::Git),
            _ => None,
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            Self::Mct => IGNORE_FILE_NAME,
            Self::Git => GITIGNORE_FILE_NAME,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Mct => "mctignore",
            Self::Git => "gitignore",
        }
    }

    fn unreadable(self) -> &'static str {
        match self {
            Self::Mct => "Cannot read .mctignore. See the server log.",
            Self::Git => "Cannot read .gitignore. See the server log.",
        }
    }
}

/// Current file text, `None` when there is no file yet.
fn read(root: &Path, rules: Rules) -> Result<Option<String>, String> {
    let name = rules.file_name();
    let path = root.join(name);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot read {name}: {error}")),
        Ok(metadata) if !metadata.is_file() => {
            return Err(format!("{name} must be a regular file."))
        }
        Ok(metadata) if metadata.len() > MAX_IGNORE_BYTES as u64 => {
            return Err(format!("{name} is larger than {MAX_IGNORE_BYTES} bytes."))
        }
        Ok(_) => {}
    }
    let bytes = fs::read(&path).map_err(|error| format!("Cannot read {name}: {error}"))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{name} is not valid UTF-8."))
}

/// Whether some line is the import directive, not commented out.
fn imports_gitignore(text: &str) -> bool {
    text.lines()
        .any(|line| line.trim() == GITIGNORE_IMPORT_DIRECTIVE)
}

fn state_json(root: &Path, rules: Rules) -> Result<Value, String> {
    let current = read(root, rules)?;
    let content = current.clone().unwrap_or_default();
    Ok(json!({
        "file": rules.id(),
        "path": rules.file_name(),
        "exists": current.is_some(),
        "content": content,
        // Only `.mctignore` has a starter text and the `@import-gitignore` directive.
        "template": if rules == Rules::Mct { IGNORE_FILE_TEMPLATE } else { "" },
        "directive": GITIGNORE_IMPORT_DIRECTIVE,
        "command": IMPORT_COMMAND,
        "importsGitignore": rules == Rules::Mct
            && content.lines().any(|line| line.trim() == GITIGNORE_IMPORT_DIRECTIVE),
        "gitignorePresent": root.join(GITIGNORE_FILE_NAME).is_file(),
        "maxBytes": MAX_IGNORE_BYTES,
    }))
}

/// Writes `content` to the rule file through a temporary file in the same directory.
fn write_atomically(root: &Path, rules: Rules, content: &str) -> std::io::Result<()> {
    let name = rules.file_name();
    let target = root.join(name);
    let temporary = root.join(format!("{name}.{}.tmp", std::process::id()));
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
    /// The state of the file the `file` parameter names (`.mctignore` by default).
    pub(super) fn ignore_state(&self, params: &Params) -> Result<Value, (u16, &'static str)> {
        let rules = Rules::named(text(params, "file")).ok_or((400, "unknown rule file"))?;
        state_json(&self.root, rules).map_err(|message| {
            eprintln!("warning: {message}");
            (500, rules.unreadable())
        })
    }

    /// Replaces a rule file with the JSON body's `content` (`csrf`, `content`,
    /// `base`, and optionally `file`: `mctignore` or `gitignore`). `base` is the
    /// text the page loaded; if the file differs now, nothing is written and
    /// 409 returns the current state.
    pub(super) fn ignore(&mut self, body: &str) -> Response {
        let Ok(Value::Object(fields)) = serde_json::from_str(body) else {
            return Response::failure(400, "invalid JSON body");
        };
        if fields.get("csrf").and_then(Value::as_str) != Some(self.csrf.as_str()) {
            return Response::error(403);
        }
        if fields
            .keys()
            .any(|key| !["csrf", "content", "base", "file"].contains(&key.as_str()))
        {
            return Response::failure(400, "unknown field");
        }
        let Some(rules) = Rules::named(fields.get("file").and_then(Value::as_str).unwrap_or(""))
        else {
            return Response::failure(400, "unknown rule file");
        };
        let name = rules.file_name();
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
        let (existed, current) = match read(&self.root, rules) {
            Ok(current) => (current.is_some(), current.unwrap_or_default()),
            Err(message) => {
                eprintln!("warning: {message}");
                return Response::failure(500, rules.unreadable());
            }
        };
        if current != base {
            return match state_json(&self.root, rules) {
                Ok(mut state) => {
                    state["error"] = format!(
                        "{name} changed on disk meanwhile. Nothing was saved; its current content is shown."
                    )
                    .into();
                    Response::json(&state).with_status(409)
                }
                Err(_) => Response::failure(500, rules.unreadable()),
            };
        }
        let mut text = content.replace("\r\n", "\n");
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        // A new `.mctignore` always carries the import directive, active.
        if rules == Rules::Mct && !existed && !imports_gitignore(&text) {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&format!("{GITIGNORE_IMPORT_DIRECTIVE}\n"));
        }
        if let Err(error) = write_atomically(&self.root, rules, &text) {
            eprintln!("warning: cannot save {name}: {error}");
            return Response::failure(
                500,
                match rules {
                    Rules::Mct => "Could not save .mctignore. No changes were applied.",
                    Rules::Git => "Could not save .gitignore. No changes were applied.",
                },
            );
        }
        match state_json(&self.root, rules) {
            Ok(state) => Response::json(&state),
            Err(_) => Response::failure(
                500,
                match rules {
                    Rules::Mct => "Saved, but cannot read .mctignore back.",
                    Rules::Git => "Saved, but cannot read .gitignore back.",
                },
            ),
        }
    }
}
