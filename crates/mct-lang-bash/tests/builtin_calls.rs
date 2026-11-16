//! D9: a call to a shell builtin (`echo`, `cd`, `exit`) that the file does
//! not define as a function can only be the shell's, so it is external, never
//! a same-named function of another script. A function the file defines under
//! the builtin's name (`echo() { … }`) is that file's own. External programs
//! (`git`, `grep`) are not builtins and are left alone.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_bash::BashParser;

const PLAIN: &str = r#"#!/usr/bin/env bash
cd "$1"
echo "start"
printf '%s\n' "$1"
git status
helper
exit 0
"#;

const SHADOWED: &str = r#"#!/usr/bin/env bash
echo() { command printf '%s\n' "$*"; }
echo "hi"
cd /tmp
"#;

fn parsed(source: &str) -> ParsedFile {
    mct_corpus::parse_source(&BashParser, "bin/a.sh", "Bash", source)
}

/// Every call as `name` plus ` external` when the parser proved it so.
fn calls(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .map(|(i, r)| {
            let external = parsed
                .relation_targets
                .iter()
                .any(|t| t.relation == i && t.external);
            format!("{}{}", r.to_name, if external { " external" } else { "" })
        })
        .collect()
}

#[test]
fn a_shell_builtin_is_external() {
    assert_eq!(
        calls(&parsed(PLAIN)),
        [
            "cd external",
            "echo external",
            "printf external",
            "git",
            "helper",
            "exit external",
        ]
    );
}

#[test]
fn a_builtin_the_file_defines_as_a_function_is_not_proven() {
    assert_eq!(
        calls(&parsed(SHADOWED)),
        ["command external", "echo", "cd external"]
    );
}
