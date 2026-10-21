//! End-to-end tests of `mct-cli call` and multi-root `init`, running the real
//! binary against throwaway projects.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A small Rust project in a fresh temp dir, removed on drop.
struct Project(PathBuf);

impl Project {
    fn new(tag: &str, function: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mct-cli-call-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("src/main.rs"),
            format!("fn {function}() {{}}\n\nfn main() {{\n    {function}();\n}}\n"),
        )
        .unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mct(args: &[&dyn AsRef<std::ffi::OsStr>]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mct-cli"))
        .args(args.iter().map(|arg| arg.as_ref()))
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn call_reports_the_indexing_status() {
    let project = Project::new("status", "alpha_one");
    let output = mct(&[
        &"--root",
        &project.path(),
        &"call",
        &"--tool",
        &"get_indexing_status",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("1 files indexed"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn call_lists_the_symbols_of_a_file() {
    let project = Project::new("symbols", "alpha_one");
    let output = mct(&[
        &"--root",
        &project.path(),
        &"call",
        &"--tool",
        &"list_symbols",
        &"--args",
        &r#"{"path":"src/main.rs"}"#,
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("alpha_one") && text.contains("main"),
        "{text}"
    );
}

#[test]
fn call_reads_its_arguments_from_a_file() {
    let project = Project::new("argsfile", "alpha_one");
    let args_file = project.path().join("args.json");
    fs::write(&args_file, r#"{"name":"alpha_one"}"#).unwrap();
    let output = mct(&[
        &"--root",
        &project.path(),
        &"call",
        &"--tool",
        &"find_symbol",
        &"--args-file",
        &args_file,
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("src/main.rs"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn call_json_prints_the_raw_mcp_result() {
    let project = Project::new("json", "alpha_one");
    let output = mct(&[
        &"--root",
        &project.path(),
        &"call",
        &"--tool",
        &"get_indexing_status",
        &"--json",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert!(value["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("1 files indexed"));
}

#[test]
fn init_and_call_cover_several_roots_with_labelled_output() {
    let first = Project::new("multi-a", "alpha_one");
    let second = Project::new("multi-b", "beta_two");
    let roots = format!("{},{}", first.path().display(), second.path().display());

    let init = mct(&[&"--roots", &roots, &"init"]);
    assert!(init.status.success(), "{}", stderr(&init));
    let text = stdout(&init);
    for project in [&first, &second] {
        assert!(
            text.contains(&format!("== {} ==", project.path().display())),
            "{text}"
        );
    }
    assert_eq!(text.matches("Index: 1 files").count(), 2, "{text}");

    // A second init finds both indexes up to date and parses nothing.
    let again = mct(&[&"--roots", &roots, &"init"]);
    assert_eq!(
        stdout(&again).matches("0 parsed, 1 unchanged").count(),
        2,
        "{}",
        stdout(&again)
    );

    // `--force` re-parses regardless.
    let forced = mct(&[&"--roots", &roots, &"init", &"--force"]);
    assert_eq!(
        stdout(&forced).matches("1 parsed, 0 unchanged").count(),
        2,
        "{}",
        stdout(&forced)
    );

    let status = mct(&[
        &"--roots",
        &roots,
        &"call",
        &"--tool",
        &"get_indexing_status",
    ]);
    assert!(status.status.success(), "{}", stderr(&status));
    let text = stdout(&status);
    for project in [&first, &second] {
        assert!(
            text.contains(&format!("== {} ==", project.path().display())),
            "{text}"
        );
    }
    assert_eq!(text.matches("1 files indexed").count(), 2, "{text}");

    // Repeated `--root` is the same as `--roots a,b`.
    let repeated = mct(&[
        &"--root",
        &first.path(),
        &"--root",
        &second.path(),
        &"call",
        &"--tool",
        &"find_symbol",
        &"--args",
        &r#"{"name":"beta_two"}"#,
    ]);
    assert!(repeated.status.success(), "{}", stderr(&repeated));
    let text = stdout(&repeated);
    assert!(text.contains("No symbol named `beta_two`"), "{text}");
    assert!(text.contains("src/main.rs"), "{text}");
}

#[test]
fn call_json_over_several_roots_is_an_array_labelled_by_root() {
    let first = Project::new("jsonmulti-a", "alpha_one");
    let second = Project::new("jsonmulti-b", "beta_two");
    let output = mct(&[
        &"--root",
        &first.path(),
        &"--root",
        &second.path(),
        &"call",
        &"--tool",
        &"get_indexing_status",
        &"--json",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    let entries = value.as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["root"], first.path().display().to_string());
    assert!(entries[1]["result"]["content"].is_array());
}

#[test]
fn call_with_an_unknown_tool_fails_and_says_so() {
    let project = Project::new("unknown", "alpha_one");
    let output = mct(&[
        &"--root",
        &project.path(),
        &"call",
        &"--tool",
        &"no_such_tool",
    ]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no tool named `no_such_tool`"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn call_with_malformed_json_arguments_fails_before_touching_the_index() {
    let project = Project::new("badjson", "alpha_one");
    let output = mct(&[
        &"--root",
        &project.path(),
        &"call",
        &"--tool",
        &"list_symbols",
        &"--args",
        &"{not json",
    ]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("not valid JSON"),
        "{}",
        stderr(&output)
    );
    assert!(!project.path().join(".mct-index").exists());

    let not_an_object = mct(&[
        &"--root",
        &project.path(),
        &"call",
        &"--tool",
        &"list_symbols",
        &"--args",
        &"[1]",
    ]);
    assert!(!not_an_object.status.success());
    assert!(stderr(&not_an_object).contains("must be a JSON object"));
}

#[test]
fn call_with_missing_required_arguments_fails() {
    let project = Project::new("noargs", "alpha_one");
    let output = mct(&[
        &"--root",
        &project.path(),
        &"call",
        &"--tool",
        &"list_symbols",
    ]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("invalid args for `list_symbols`"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_missing_root_fails_for_call_and_init() {
    let missing = std::env::temp_dir().join(format!("mct-cli-call-missing-{}", std::process::id()));
    let call = mct(&[
        &"--root",
        &missing,
        &"call",
        &"--tool",
        &"get_indexing_status",
    ]);
    let init = mct(&[&"--root", &missing, &"init"]);
    for output in [call, init] {
        assert!(!output.status.success());
        assert!(
            stderr(&output).contains("does not exist"),
            "{}",
            stderr(&output)
        );
    }
    assert!(!missing.exists());
}

#[test]
fn several_roots_are_rejected_by_single_root_commands() {
    let first = Project::new("single-a", "alpha_one");
    let second = Project::new("single-b", "beta_two");
    let output = mct(&[
        &"--root",
        &first.path(),
        &"--root",
        &second.path(),
        &"status",
    ]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("exactly one --root"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn obsidian_flags_manual_command_and_warning_exit_status() {
    let project = Project::new("obsidian", "atlas_entry");
    let root = project.path();
    let vault = root.join(".mct-index/obsidian");
    let output = mct(&[&"--root", &root, &"init", &"--no-obsidian"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!vault.exists());
    let output = mct(&[&"--root", &root, &"reindex", &"--no-obsidian"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!vault.exists());
    let output = mct(&[&"--root", &root, &"obsidian"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(vault.join("index.md").is_file());
    assert!(stdout(&output).contains("written"));
    let before = fs::read(vault.join("index.md")).unwrap();
    fs::write(root.join("src/new.rs"), "pub fn newly_added() {}\n").unwrap();
    let output = mct(&[&"--root", &root, &"reindex", &"--no-obsidian"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read(vault.join("index.md")).unwrap(), before);
    let output = mct(&[&"--root", &root, &"reindex"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("Obsidian vault:"));
    // A user edit must be protected and reported without failing indexing.
    fs::write(vault.join("index.md"), "user edit").unwrap();
    let output = mct(&[&"--root", &root, &"reindex"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stderr(&output).contains("warning:"));
    let output = mct(&[&"--root", &root, &"obsidian"]);
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(vault.join("index.md")).unwrap(),
        "user edit"
    );
}
