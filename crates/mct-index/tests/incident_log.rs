//! The local incident history in `<root>/.mct-index/errors/`: the writer's
//! safety rules and what a reindex records. The operation's own result is
//! always checked next to the files.

// Test code: an unwrap()/expect() here means a broken test precondition.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use mct_index::{log_incident, ExcludeSet, Incident, Index, IndexError};

#[path = "common/fake_parser.rs"]
mod fake_parser;
use fake_parser::registry;

#[path = "common/scratch_dir.rs"]
mod scratch;
use scratch::scratch_dir;

#[path = "common/write.rs"]
mod fixture_files;
use fixture_files::write;

/// A project with an existing `.mct-index/` (what `init` leaves behind).
fn project() -> PathBuf {
    let root = scratch_dir("mct-incident-log");
    fs::create_dir(root.join(".mct-index")).unwrap();
    root
}

fn errors_dir(root: &Path) -> PathBuf {
    root.join(".mct-index").join("errors")
}

/// The published records, sorted; temporaries and other files are not records.
fn records(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(errors_dir(root)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn read(root: &Path, name: &str) -> String {
    fs::read_to_string(errors_dir(root).join(name)).unwrap()
}

fn incident<'a>(operation: &'a str, kind: &'a str, detail: &'a str) -> Incident<'a> {
    Incident {
        operation,
        kind,
        path: None,
        line: None,
        detail,
    }
}

fn open(root: &Path) -> Index {
    Index::open_in_memory(root, ExcludeSet::default()).unwrap()
}

// ---- the writer --------------------------------------------------------

#[test]
fn a_record_has_a_dated_name_and_the_selected_fields() {
    let root = project();
    let published = log_incident(
        &root,
        &Incident {
            operation: "reindex",
            kind: "syntax-error",
            path: Some("src/a.py"),
            line: Some(7),
            detail: "unexpected indent",
        },
    )
    .unwrap()
    .unwrap();

    let name = published
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let parts: Vec<&str> = name.split("--").collect();
    assert_eq!(parts.len(), 4, "{name}");
    let stamp = parts[0];
    assert_eq!(stamp.len(), "20261119T153045.123456Z".len(), "{name}");
    assert!(stamp.ends_with('Z') && stamp.as_bytes()[8] == b'T' && stamp.as_bytes()[15] == b'.');
    assert_eq!((parts[1], parts[2]), ("reindex", "syntax-error"));
    let id = parts[3].strip_suffix(".log").unwrap();
    assert_eq!(id.len(), 12);
    assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));

    let body = fs::read_to_string(&published).unwrap();
    assert!(body.starts_with("date: 20") && body.contains('Z'), "{body}");
    assert!(body.contains("\noperation: reindex\n"), "{body}");
    assert!(body.contains("\nkind: syntax-error\n"), "{body}");
    assert!(body.contains("\npath: src/a.py\n"), "{body}");
    assert!(body.contains("\nline: 7\n"), "{body}");
    assert!(body.ends_with("detail: unexpected indent\n"), "{body}");
    // Only the record remains: no temporary file is left behind.
    assert_eq!(records(&root), vec![name]);
}

#[test]
fn a_hostile_operation_and_a_dirty_diagnostic_are_sanitized() {
    let root = project();
    let secret_line = format!(
        "{}/secret.txt failed\nforged: line password=hunter2 \u{7}end",
        root.display()
    );
    let published = log_incident(
        &root,
        &incident("../../evil\u{0}/op", "Kind With Spaces", &secret_line),
    )
    .unwrap()
    .unwrap();

    assert_eq!(published.parent().unwrap(), errors_dir(&root));
    let name = published
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(name.contains("--evil-op--kind-with-spaces--"), "{name}");
    let body = fs::read_to_string(&published).unwrap();
    // One `detail:` line: the newline cannot forge another field.
    assert_eq!(body.lines().filter(|l| l.starts_with("forged")).count(), 0);
    assert!(
        body.contains("detail: <root>/secret.txt failed forged: line "),
        "{body}"
    );
    assert!(body.contains("password=<redacted>"), "{body}");
    assert!(
        !body.contains("hunter2") && !body.contains('\u{7}'),
        "{body}"
    );
    assert!(!body.contains(root.to_str().unwrap()), "{body}");
}

#[test]
fn nothing_is_written_without_an_index_directory() {
    let root = scratch_dir("mct-incident-none");
    assert!(log_incident(&root, &incident("call", "cli-error", "boom"))
        .unwrap()
        .is_none());
    assert!(!root.join(".mct-index").exists(), "no directory may appear");

    // `.mct-index` as a regular file is not an index directory either.
    fs::write(root.join(".mct-index"), "not a directory").unwrap();
    assert!(log_incident(&root, &incident("call", "cli-error", "boom"))
        .unwrap()
        .is_none());
    assert!(root.join(".mct-index").is_file());
}

#[cfg(unix)]
#[test]
fn symlinks_are_never_followed() {
    use std::os::unix::fs::symlink;

    // `errors` is a symlink to a directory elsewhere.
    let root = project();
    let elsewhere = scratch_dir("mct-incident-target");
    symlink(&elsewhere, errors_dir(&root)).unwrap();
    assert!(log_incident(&root, &incident("op", "kind", "d")).is_err());
    assert_eq!(
        fs::read_dir(&elsewhere).unwrap().count(),
        0,
        "target untouched"
    );

    // `.mct-index` is a symlink: treated as missing, nothing written through it.
    let root = scratch_dir("mct-incident-linked");
    let real = scratch_dir("mct-incident-real");
    symlink(&real, root.join(".mct-index")).unwrap();
    assert!(log_incident(&root, &incident("op", "kind", "d"))
        .unwrap()
        .is_none());
    assert_eq!(fs::read_dir(&real).unwrap().count(), 0, "target untouched");
}

#[test]
fn a_regular_file_where_errors_should_be_blocks_the_write_quietly() {
    let root = project();
    fs::write(errors_dir(&root), "occupied").unwrap();
    let error = log_incident(&root, &incident("op", "kind", "d")).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(fs::read_to_string(errors_dir(&root)).unwrap(), "occupied");
}

#[test]
fn concurrent_incidents_each_get_their_own_file() {
    let root = project();
    let threads = 16;
    let per_thread = 8;
    std::thread::scope(|scope| {
        for t in 0..threads {
            let root = &root;
            scope.spawn(move || {
                for n in 0..per_thread {
                    let detail = format!("thread {t} incident {n}");
                    let path = log_incident(root, &incident("batch", "internal-error", &detail))
                        .unwrap()
                        .unwrap();
                    assert!(fs::read_to_string(path).unwrap().contains(&detail));
                }
            });
        }
    });
    let names = records(&root);
    assert_eq!(names.len(), threads * per_thread);
    let unique: HashSet<&String> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "no record was overwritten");
    assert!(
        names.iter().all(|n| n.ends_with(".log")),
        "no temporary left"
    );
}

// ---- what a reindex records -------------------------------------------

#[test]
fn a_clean_reindex_records_nothing() {
    let root = project();
    write(&root, "a.fake", "fn alpha\n");
    let mut index = open(&root);
    let report = index.reindex(&registry(), false).unwrap();
    assert!(report.issues.is_empty());
    assert!(records(&root).is_empty());
    assert!(
        !errors_dir(&root).exists(),
        "errors/ appears on demand only"
    );
}

#[test]
fn a_syntax_error_is_recorded_once_per_distinct_state() {
    let root = project();
    write(&root, "good.fake", "fn ok\n");
    let bad = write(&root, "bad.fake", "fn fine\nnot valid\n");
    let mut index = open(&root);

    // First detection: the report is the usual one and one record exists.
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 1);
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].relative_path, "bad.fake");
    let names = records(&root);
    assert_eq!(names.len(), 1);
    assert!(
        names[0].contains("--reindex--syntax-error--"),
        "{}",
        names[0]
    );
    let body = read(&root, &names[0]);
    assert!(body.contains("\npath: bad.fake\n"), "{body}");
    assert!(body.contains("\nline: 2\n"), "{body}");
    assert!(body.contains("expected `fn`, got `not valid`"), "{body}");

    // The same invalid content on later runs (the file is re-parsed each
    // time) and a forced one add nothing, and still report the issue.
    for force in [false, false, true] {
        let report = index.reindex(&registry(), force).unwrap();
        assert_eq!(report.issues.len(), 1);
    }
    assert_eq!(records(&root).len(), 1);

    // New invalid content with another diagnostic is a new occurrence.
    fs::write(&bad, "fn fine\nanother problem\n").unwrap();
    index.reindex(&registry(), false).unwrap();
    assert_eq!(records(&root).len(), 2);

    // A recovery records nothing...
    fs::write(&bad, "fn fine\n").unwrap();
    let report = index.reindex(&registry(), false).unwrap();
    assert!(report.issues.is_empty());
    assert_eq!(records(&root).len(), 2);

    // ...and breaking the file again after it recovered records again.
    fs::write(&bad, "fn fine\nanother problem\n").unwrap();
    index.reindex(&registry(), false).unwrap();
    assert_eq!(records(&root).len(), 3);
}

#[test]
fn reindex_requested_records_an_issue_once() {
    let root = project();
    write(&root, "bad.fake", "oops\n");
    let mut index = open(&root);
    index.reindex_requested(&registry(), false).unwrap();
    assert_eq!(records(&root).len(), 1);
}

#[test]
fn an_incremental_reindex_records_what_it_finds() {
    let root = project();
    write(&root, "ok.fake", "fn ok\n");
    let mut index = open(&root);
    index.reindex(&registry(), false).unwrap();
    assert!(records(&root).is_empty());

    let bad = write(&root, "later.fake", "broken\n");
    let report = index.reindex_paths(&registry(), &[bad]).unwrap();
    assert_eq!(report.issues.len(), 1);
    let names = records(&root);
    assert_eq!(names.len(), 1);
    assert!(read(&root, &names[0]).contains("\npath: later.fake\n"));
}

#[test]
fn a_project_without_an_index_directory_keeps_its_report_and_gets_no_directory() {
    let root = scratch_dir("mct-incident-bare");
    write(&root, "bad.fake", "oops\n");
    let mut index = open(&root);
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.issues.len(), 1);
    assert!(!root.join(".mct-index").exists());
}

#[test]
fn a_blocked_log_directory_does_not_change_the_reindex_result() {
    let blocked = project();
    fs::write(errors_dir(&blocked), "occupied").unwrap();
    write(&blocked, "bad.fake", "oops\n");
    write(&blocked, "good.fake", "fn ok\n");

    let usable = project();
    write(&usable, "bad.fake", "oops\n");
    write(&usable, "good.fake", "fn ok\n");

    let with_block = open(&blocked).reindex(&registry(), false).unwrap();
    let without_block = open(&usable).reindex(&registry(), false).unwrap();
    assert_eq!(with_block.files_parsed, without_block.files_parsed);
    assert_eq!(with_block.symbols_written, without_block.symbols_written);
    assert_eq!(with_block.issues.len(), without_block.issues.len());
    assert_eq!(with_block.issues[0].detail, without_block.issues[0].detail);
    assert_eq!(
        fs::read_to_string(errors_dir(&blocked)).unwrap(),
        "occupied"
    );
    assert_eq!(records(&usable).len(), 1);
}

#[test]
fn a_failing_post_reindex_hook_is_recorded_and_still_a_warning() {
    let root = project();
    write(&root, "a.fake", "fn alpha\n");
    let mut index = open(&root);
    index.set_post_reindex_hook(Box::new(|_| Err(IndexError::Hook("disk full".into()))));
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("disk full"));
    let names = records(&root);
    assert_eq!(names.len(), 1);
    assert!(
        names[0].contains("--reindex--post-reindex-hook--"),
        "{}",
        names[0]
    );
    assert!(read(&root, &names[0]).contains("disk full"));
}

#[cfg(unix)]
#[test]
fn an_unreadable_file_is_recorded_on_every_occurrence() {
    use std::os::unix::fs::PermissionsExt;

    let root = project();
    let secret = write(&root, "locked.fake", "fn hidden\n");
    let mut index = open(&root);
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).unwrap();
    let enforced = fs::read(&secret).is_err();
    if !enforced {
        eprintln!("skipping: permission denial is not enforced for this process");
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();
        return;
    }
    for expected in 1..=2 {
        let report = index.reindex(&registry(), false).unwrap();
        assert_eq!(report.issues.len(), 1);
        assert_eq!(records(&root).len(), expected);
    }
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();
    let names = records(&root);
    assert!(names
        .iter()
        .all(|n| n.contains("--reindex--read-failure--")));
}

#[test]
fn the_log_directory_is_not_indexed() {
    let root = project();
    write(&root, "a.fake", "fn alpha\n");
    // A record that looks like source must not be picked up as a project file.
    write(
        &root,
        ".mct-index/errors/looks-like-source.fake",
        "not valid\n",
    );
    let mut index = open(&root);
    let report = index.reindex(&registry(), false).unwrap();
    assert_eq!(report.files_parsed, 1);
    assert!(report.issues.is_empty());
}
