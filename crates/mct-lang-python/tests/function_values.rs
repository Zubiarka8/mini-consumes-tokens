//! A function or class used as a value (`safely(score, report)`,
//! `sorted(rows, key=rank)`, `isinstance(x, (A, B))`, `handler = rank`,
//! `{"k": rank}`, `return score`) is a use of it: a `references` relation, with
//! the evidence a bare call to that name gets (this file's `def`, an import,
//! a builtin). A name with no such evidence (a parameter, a local, an
//! unbound name) records nothing, so it is never resolved by its name alone.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, RelationKind};
use mct_lang_python::PythonParser;

const SOURCE: &str = r#"from app.util import parse


def score(report):
    return report


def rank(row):
    return row


def safely(fn, *args):
    return fn(*args)


def run(report, rows, handler):
    safely(score, report)
    sorted(rows, key=rank)
    safely(parse, report)
    map(str, rows)
    safely(handler, report)
    safely(unknown, report)
    isinstance(report, (Score, list))
    handler = rank
    table = {"rank": rank, "keys": [score]}
    return score


class Score:
    pass
"#;

/// Every `references` relation as `name` plus its evidence.
fn references(parsed: &ParsedFile) -> Vec<String> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::References)
        .map(|(i, r)| {
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            let evidence = match target {
                Some(t) if t.external => " external".to_string(),
                Some(t) => match (&t.path, &t.module) {
                    (Some(path), _) => format!(" path={path}"),
                    (_, Some(module)) => format!(" module={module}"),
                    _ => String::new(),
                },
                None => String::new(),
            };
            format!("{}:{}{evidence}", r.to_name, r.location.line)
        })
        .collect()
}

#[test]
fn a_function_passed_as_an_argument_is_referenced_with_evidence() {
    let parsed = mct_corpus::parse_source(&PythonParser, "app/run.py", "Python", SOURCE);
    assert_eq!(
        references(&parsed),
        [
            "score:17 path=app/run.py",
            "rank:18 path=app/run.py",
            "parse:19 module=util",
            "str:20 external",
            "Score:23 path=app/run.py",
            "list:23 external",
            "rank:24 path=app/run.py",
            "rank:25 path=app/run.py",
            "score:25 path=app/run.py",
            "score:26 path=app/run.py",
        ]
    );
}

#[test]
fn nested_calls_preserve_evidence_for_each_argument() {
    let source = "from app.util import parse\ndef score(x): return x\ndef run():\n    consume(score, wrap(parse), again=score, builtin=str)\n";
    let parsed = mct_corpus::parse_source(&PythonParser, "app/run.py", "Python", source);
    assert_eq!(
        references(&parsed),
        [
            "score:4 path=app/run.py",
            "score:4 path=app/run.py",
            "str:4 external",
            "parse:4 module=util",
        ]
    );
}

#[test]
fn nested_values_keep_evidence_and_ignore_unknown_names() {
    let source = "from app.util import parse\ndef score(x): return x\ndef run(unknown):\n    return [score, (parse, {\"key\": [str, unknown]})]\n";
    let parsed = mct_corpus::parse_source(&PythonParser, "app/run.py", "Python", source);
    assert_eq!(
        references(&parsed),
        [
            "score:4 path=app/run.py",
            "parse:4 module=util",
            "str:4 external",
        ]
    );
}
