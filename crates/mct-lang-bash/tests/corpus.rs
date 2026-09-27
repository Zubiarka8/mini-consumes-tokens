//! Long, multi-file fixture corpus (issue #74): a tar-based backup tool
//! split into `lib/log.sh`, `lib/config.sh`, `lib/fs.sh`, `lib/remote.sh`
//! and the `bin/backup.sh` entry point, each 300–600 lines and `source`-ing
//! the others. The shared checks (size, line ranges, golden snapshot, index
//! round trip, malformed input) come from `mct-corpus`; the tests below pin
//! the constructs and cross-file relations this language is expected to
//! extract.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-bash/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_bash::BashParser;

mct_corpus::standard_tests!(BashParser);

fn lines(path: &str, name: &str, kind: SymbolKind) -> (u32, Option<u32>) {
    let s = corpus().symbol(path, name, kind);
    (s.location.line, s.location.end_line)
}

#[test]
fn functions_in_every_definition_form_are_extracted() {
    use SymbolKind::Function;
    // `name() { ... }`
    assert_eq!(lines("lib/log.sh", "log", Function), (79, Some(93)));
    assert_eq!(lines("lib/log.sh", "retry", Function), (205, Some(218)));
    // One-line bodies.
    lines("lib/log.sh", "log_info", Function);
    // `function name { ... }` without parentheses.
    lines("lib/fs.sh", "snapshot_btrfs", Function);
    lines("lib/remote.sh", "start_ssh_agent", Function);
    lines("bin/backup.sh", "cmd_version", Function);
    // `name() ( ... )` — a subshell body.
    lines("lib/fs.sh", "tar_in_dir", Function);
    // Every top-level function's parent is its file's module.
    assert_eq!(
        corpus()
            .symbol("bin/backup.sh", "main", Function)
            .parent
            .as_deref(),
        Some("backup")
    );
}

#[test]
fn nested_function_definitions_belong_to_the_enclosing_function() {
    let c = corpus();
    let inner = c.symbol("lib/fs.sh", "btrfs_available", SymbolKind::Function);
    assert_eq!(inner.parent.as_deref(), Some("snapshot_btrfs"));
    assert_eq!(
        lines("lib/fs.sh", "btrfs_available", SymbolKind::Function),
        (281, Some(283))
    );
    c.relation(
        "lib/fs.sh",
        "snapshot_btrfs",
        RelationKind::Calls,
        "btrfs_available",
    );
    c.relation("lib/fs.sh", "btrfs_available", RelationKind::Calls, "btrfs");
}

#[test]
fn long_bodies_keep_exact_line_ranges() {
    use SymbolKind::{Function, Module};
    assert_eq!(
        lines("bin/backup.sh", "parse_args", Function),
        (68, Some(128))
    );
    assert_eq!(lines("bin/backup.sh", "main", Function), (279, Some(289)));
    assert_eq!(
        lines("lib/config.sh", "load_config_file", Function),
        (119, Some(140))
    );
    // The file-level module ends on the file's last line.
    assert_eq!(lines("lib/log.sh", "log", Module), (1, Some(321)));
    assert_eq!(lines("bin/backup.sh", "backup", Module), (1, Some(303)));
}

#[test]
fn top_level_assignments_are_variables_and_locals_are_not() {
    use SymbolKind::Variable;
    let c = corpus();
    // Plain, `declare -A`, `declare -g`, `declare -i`, `readonly -a` forms.
    assert_eq!(lines("lib/config.sh", "CONFIG", Variable), (21, Some(36)));
    lines("lib/config.sh", "KNOWN_COMPRESSIONS", Variable);
    lines("lib/remote.sh", "REMOTE_KIND", Variable);
    lines("lib/fs.sh", "FILES_SEEN", Variable);
    lines("lib/log.sh", "__TIMERS", Variable);
    // Assigned in both branches of a top-level `if`: two definitions.
    assert_eq!(c.symbols_named("C_RED").len(), 2);
    // `local`/function-scoped assignments are not symbols.
    for local in ["level", "attempts", "archive", "DRY_RUN_FLAG"] {
        assert!(
            c.symbols_named(local).is_empty(),
            "{local} is function-local and should not be a symbol"
        );
    }
    // A command's prefix assignment (`LC_ALL=C TZ=UTC main "$@"`) only sets
    // that command's environment.
    assert!(c.symbols_named("LC_ALL").is_empty());
    assert!(c.symbols_named("TZ").is_empty());
    c.relation("bin/backup.sh", "backup", RelationKind::Calls, "main");
}

#[test]
fn source_and_dot_are_imports() {
    let c = corpus();
    c.relation(
        "bin/backup.sh",
        "backup",
        RelationKind::Imports,
        "$LIB_DIR/log.sh",
    );
    // `.` is the same as `source`.
    c.relation(
        "bin/backup.sh",
        "backup",
        RelationKind::Imports,
        "$LIB_DIR/fs.sh",
    );
    c.relation(
        "lib/fs.sh",
        "fs",
        RelationKind::Imports,
        "$FS_DIR/config.sh",
    );
    c.relation(
        "lib/remote.sh",
        "remote",
        RelationKind::Imports,
        "$REMOTE_DIR/fs.sh",
    );
}

#[test]
fn calls_in_substitutions_pipelines_and_compound_commands_are_extracted() {
    let c = corpus();
    // `$(...)` inside an argument.
    c.relation("lib/log.sh", "log", RelationKind::Calls, "colour_for");
    // Pipelines, including a pipeline into `case`.
    c.relation("lib/fs.sh", "restore_archive", RelationKind::Calls, "zstd");
    c.relation("lib/fs.sh", "restore_archive", RelationKind::Calls, "tar");
    // Process substitution `< <(...)`, here-string and `>(...)`.
    c.relation(
        "lib/config.sh",
        "load_env_overrides",
        RelationKind::Calls,
        "env",
    );
    c.relation("lib/fs.sh", "count_matching", RelationKind::Calls, "wc");
    // Inside a subshell `( ... )`.
    c.relation("lib/log.sh", "with_lock", RelationKind::Calls, "flock");
    // `until "$@"` / `"$fn"` have no static name and are skipped.
    assert!(!c
        .relations()
        .iter()
        .any(|r| r.kind == RelationKind::Calls && r.to.starts_with('"')));
}

#[test]
fn cross_file_calls_are_extracted() {
    let c = corpus();
    c.relation(
        "bin/backup.sh",
        "cmd_run",
        RelationKind::Calls,
        "create_archive",
    );
    c.relation("bin/backup.sh", "cmd_run", RelationKind::Calls, "upload");
    c.relation(
        "bin/backup.sh",
        "setup",
        RelationKind::Calls,
        "install_traps",
    );
    c.relation("lib/remote.sh", "upload_ssh", RelationKind::Calls, "retry");
    c.relation(
        "lib/remote.sh",
        "remote_verify",
        RelationKind::Calls,
        "checksum",
    );
    c.relation(
        "lib/fs.sh",
        "archive_name",
        RelationKind::Calls,
        "compression_extension",
    );
    c.relation(
        "lib/config.sh",
        "print_config",
        RelationKind::Calls,
        "summary_table",
    );
    assert!(c.cross_file_relation_count() >= 100);
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    let callers = index.find_callers("config_get").unwrap();
    for file in [
        "lib/fs.sh",
        "lib/remote.sh",
        "bin/backup.sh",
        "lib/config.sh",
    ] {
        assert!(
            callers.iter().any(|h| h.relative_path == file),
            "config_get should be called from {file}"
        );
    }
    let calls = index.find_calls("cmd_run").unwrap();
    for callee in [
        "collect_files",
        "write_manifest",
        "apply_retention",
        "notify",
    ] {
        assert!(
            calls.iter().any(|h| h.to_name == callee),
            "cmd_run should call {callee}"
        );
    }
    let hits = index.find_symbol("die").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].relative_path, "lib/log.sh");
}

#[test]
#[ignore = "#79: tree-sitter-bash 0.25 misparses `coproc NAME { ... }`"]
fn named_coproc_keeps_the_enclosing_function_intact() {
    let c = corpus();
    assert_eq!(
        lines("lib/fs.sh", "hash_many", SymbolKind::Function),
        (225, Some(236))
    );
    c.relation("lib/fs.sh", "hash_many", RelationKind::Calls, "xargs");
    c.relation("lib/fs.sh", "hash_many", RelationKind::Calls, "wait");
    assert!(!c.has_relation("fs", RelationKind::Calls, "}"));
}
