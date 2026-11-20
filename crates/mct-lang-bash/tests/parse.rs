// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-bash/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};
use mct_lang_bash::BashParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    mct_corpus::parse_source(&BashParser, "deploy.sh", "bash", src)
}

#[test]
fn extracts_function_and_call() {
    let parsed =
        parse("helper() {\n  log \"called\"\n}\n\ngreet() {\n  helper\n  echo \"hi\"\n}\n");
    let greet = parsed.symbols.iter().find(|s| s.name == "greet").unwrap();
    assert_eq!(greet.kind, SymbolKind::Function);
    assert_eq!(
        greet.parent.as_deref(),
        Some("deploy"),
        "top-level function's parent is its module (file)"
    );

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"helper"));
    assert!(
        calls.contains(&"echo"),
        "external commands are recorded too, as unresolved calls"
    );
}

#[test]
fn top_level_variable_assignment_is_indexed() {
    let parsed = parse("FOO=bar\nexport BAZ=\"qux\"\n");
    mct_corpus::assert_variables(
        &parsed,
        &["FOO", "BAZ"],
        "export-wrapped assignment is still indexed",
    );
}

#[test]
fn variable_assigned_inside_a_function_is_not_indexed() {
    let parsed = parse("run() {\n  local x=1\n}\n");
    mct_corpus::assert_no_variables(&parsed);
}

#[test]
fn source_with_literal_path_is_an_import() {
    let parsed = parse("source ./lib.sh\n. \"$HOME/.profile\"\n");
    mct_corpus::assert_imports(&parsed, &["./lib.sh", "$HOME/.profile"]);
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    mct_corpus::assert_syntax_error_reported(&BashParser, "broken.sh", "function foo( {\n");
}

#[test]
fn quoted_alternative_accepts_literal_semicolons_and_preserves_locations() {
    let parsed =
        parse("warn() { echo ok; }\nvalue=\"${stale:+$stale; }${other:+$other; }check\"\nwarn\n");
    let value = parsed
        .symbols
        .iter()
        .find(|symbol| symbol.name == "value")
        .unwrap();
    assert_eq!(value.location.line, 2);
    let warn_call = parsed
        .relations
        .iter()
        .find(|relation| relation.to_name == "warn")
        .unwrap();
    assert_eq!(warn_call.location.line, 3);
    assert_eq!(warn_call.location.column, 1);
}

#[test]
fn pr_body_script_parses_without_execution() {
    let parsed = parse(include_str!("../../../scripts/unix/pr-body.sh"));
    assert!(parsed
        .symbols
        .iter()
        .any(|symbol| symbol.name == "stamp_of"));
    assert!(parsed
        .relations
        .iter()
        .any(|relation| relation.to_name == "warn"));
}

#[test]
fn quoted_alternative_preserves_nested_command_substitution_calls() {
    let parsed = parse("value=\"${stale:+$(echo ok;) ; }check\"\n");
    let call = parsed
        .relations
        .iter()
        .find(|relation| relation.to_name == "echo")
        .unwrap();
    assert_eq!(call.location.line, 1);
    assert_eq!(call.location.column, 19);
}

#[test]
fn quoted_alternative_extracts_original_import_text() {
    let parsed = parse("source \"${stale:+$stale; }lib.sh\"\n");
    assert!(parsed.relations.iter().any(|relation| {
        relation.kind == RelationKind::Imports && relation.to_name == "${stale:+$stale; }lib.sh"
    }));
}

#[test]
fn alternative_workaround_preserves_legitimate_syntax_errors() {
    for source in [
        "value=\"${stale:+$stale; }check\"\necho |\n",
        "value=\"${stale:+$stale; }check\"\nfunction broken( {\n",
        "value=\"${stale:+$(echo |) ; }check\"\n",
        "value=\"${stale:+$stale; check\"\n",
        "value=\"${stale:+$stale; }check\n",
        "value=\"${:+$stale; }check\"\n",
    ] {
        assert!(
            matches!(
                BashParser.parse(&SourceFile {
                    relative_path: "broken.sh".to_string(),
                    contents: source.to_string(),
                }),
                Err(mct_core::ParseError::Syntax { .. })
            ),
            "invalid source was accepted: {source}"
        );
    }
}

/// Issue #79: tree-sitter-bash 0.25 reads `coproc [NAME] { …; }` as one
/// plain command whose `;` and `}` close the enclosing function. The parser
/// reparses with `coproc [NAME]` blanked, so the body is a brace group.
#[test]
fn a_brace_coproc_keeps_the_enclosing_function_intact() {
    for coproc in ["coproc HASHER", "coproc"] {
        let src = format!(
            "hash_many() {{\n    local file\n    {coproc} {{ xargs -0 sha256sum; }}\n    printf x\n    wait \"$HASHER_PID\"\n}}\nafter\n"
        );
        let parsed = parse(&src);
        let f = parsed
            .symbols
            .iter()
            .find(|s| s.name == "hash_many")
            .unwrap();
        assert_eq!(
            (f.location.line, f.location.end_line),
            (1, Some(6)),
            "{coproc}"
        );
        let calls_of = |id| {
            parsed
                .relations
                .iter()
                .filter(|r| r.from == id && r.kind == RelationKind::Calls)
                .map(|r| (r.to_name.as_str(), r.location.line, r.location.column))
                .collect::<Vec<_>>()
        };
        let calls = calls_of(f.id);
        // 4 spaces of indent, the prefix, then ` { ` before `xargs`.
        let column = coproc.len() as u32 + 8;
        assert!(calls.contains(&("xargs", 3, column)), "{coproc}: {calls:?}");
        for callee in ["printf", "wait"] {
            assert!(
                calls.iter().any(|c| c.0 == callee),
                "{coproc}: {callee} {calls:?}"
            );
        }
        assert!(calls.iter().all(|c| c.0 != "coproc"), "{coproc}: {calls:?}");
        assert!(
            parsed.relations.iter().all(|r| r.to_name != "}"),
            "{coproc}"
        );
    }
}

/// A coproc of a simple command (`coproc cat file`) is unaffected.
#[test]
fn a_simple_command_coproc_parses_as_before() {
    let parsed = parse("run() {\n    coproc cat file\n    wait\n}\n");
    let f = parsed.symbols.iter().find(|s| s.name == "run").unwrap();
    assert_eq!(f.location.end_line, Some(4));
}
