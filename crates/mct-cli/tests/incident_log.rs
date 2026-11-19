//! `mct-cli` failures are recorded in `<root>/.mct-index/errors/` while stdout,
//! stderr and the exit status stay exactly what they are without the log.
//! Runs the real binary against throwaway projects.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

#[path = "common/command.rs"]
mod command;
use command::mct;

/// A project in a fresh temp dir, removed on drop.
struct Project(PathBuf);

impl Project {
    /// `init`ed, so `.mct-index/` exists; `extra` files are written first.
    fn indexed(tag: &str, extra: &[(&str, &str)]) -> Self {
        let dir =
            std::env::temp_dir().join(format!("mct-cli-incident-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("src/main.rs"),
            "fn alpha() {}\n\nfn main() {\n    alpha();\n}\n",
        )
        .unwrap();
        for (name, text) in extra {
            fs::write(dir.join(name), text).unwrap();
        }
        let init = mct(&[&"--root", &dir, &"init", &"--no-obsidian"]);
        assert!(init.status.success(), "{}", stderr(&init));
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn errors(&self) -> PathBuf {
        self.0.join(".mct-index").join("errors")
    }

    /// Makes writing a record impossible: a regular file where `errors/` goes.
    fn block_log(&self) {
        let _ = fs::remove_dir_all(self.errors());
        fs::write(self.errors(), "occupied").unwrap();
    }

    fn records(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.errors()) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.errors().join(name)).unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_failed_call_is_recorded_and_prints_what_it_always_printed() {
    let usable = Project::indexed("call-ok-log", &[]);
    let blocked = Project::indexed("call-blocked-log", &[]);
    blocked.block_log();
    let run = |project: &Project| {
        mct(&[
            &"--root",
            &project.path(),
            &"call",
            &"--tool",
            &"no_such_tool",
        ])
    };

    let recorded = run(&usable);
    let silent = run(&blocked);

    assert!(!recorded.status.success());
    assert!(
        stderr(&recorded).contains("call `no_such_tool` failed"),
        "{}",
        stderr(&recorded)
    );
    assert_eq!(recorded.status.code(), silent.status.code());
    assert_eq!(stderr(&recorded), stderr(&silent));
    assert_eq!(stdout(&recorded), stdout(&silent));
    assert_eq!(fs::read_to_string(blocked.errors()).unwrap(), "occupied");

    let names = usable.records();
    assert!(
        names.iter().any(|n| n.contains("--call--cli-error--")),
        "{names:?}"
    );
    let body = usable.read(&names[0]);
    assert!(body.contains("no_such_tool"), "{body}");
}

#[test]
fn malformed_arguments_are_recorded_only_when_the_project_has_an_index() {
    let indexed = Project::indexed("json-indexed", &[]);
    let output = mct(&[
        &"--root",
        &indexed.path(),
        &"call",
        &"--tool",
        &"find_symbol",
        &"--args",
        &r#"{"name": "#,
    ]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("tool arguments are not valid JSON"),
        "{}",
        stderr(&output)
    );
    let names = indexed.records();
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(names[0].contains("--call--cli-error--"), "{}", names[0]);

    // No index yet: the usual diagnostic, and no directory appears.
    let bare = std::env::temp_dir().join(format!("mct-cli-incident-bare-{}", std::process::id()));
    let _ = fs::remove_dir_all(&bare);
    fs::create_dir_all(&bare).unwrap();
    let output = mct(&[
        &"--root",
        &bare,
        &"call",
        &"--tool",
        &"find_symbol",
        &"--args",
        &r#"{"name": "#,
    ]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("tool arguments are not valid JSON"),
        "{}",
        stderr(&output)
    );
    assert!(!bare.join(".mct-index").exists());
    let _ = fs::remove_dir_all(&bare);
}

#[test]
fn a_reindex_with_a_syntax_error_succeeds_and_records_it_once() {
    // `init` indexes a broken Rust file: that reports the problem and records it.
    let project = Project::indexed("syntax", &[("src/broken.rs", "fn broken( {\n")]);
    let names = project.records();
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(
        names[0].contains("--reindex--syntax-error--"),
        "{}",
        names[0]
    );
    let body = project.read(&names[0]);
    assert!(body.contains("\npath: src/broken.rs\n"), "{body}");
    assert!(body.contains("\nline: 1\n"), "{body}");

    // Reindexing the same broken file keeps succeeding and adds nothing.
    let again = mct(&[&"--root", &project.path(), &"reindex", &"--no-obsidian"]);
    assert!(again.status.success(), "{}", stderr(&again));
    assert!(
        stdout(&again).contains("broken.rs"),
        "the report still names it: {}",
        stdout(&again)
    );
    assert_eq!(project.records().len(), 1);

    // Same command with the log blocked: the same output and exit status.
    let blocked = Project::indexed("syntax-blocked", &[("src/broken.rs", "fn broken( {\n")]);
    blocked.block_log();
    let silent = mct(&[&"--root", &blocked.path(), &"reindex", &"--no-obsidian"]);
    assert_eq!(again.status.code(), silent.status.code());
    assert_eq!(stderr(&again), stderr(&silent));
    assert_eq!(fs::read_to_string(blocked.errors()).unwrap(), "occupied");
}
