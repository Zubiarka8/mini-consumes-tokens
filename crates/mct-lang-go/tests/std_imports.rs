//! D9: `pkg.F(…)` through an import of a Go standard package (`errors`,
//! `encoding/json` as `json`, an alias) is external, never a same-named
//! method or function in the repository. A name the file also uses other
//! than as a selector operand (a local `path := …`, a parameter) is not
//! proven, and neither is an import of a repository package.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_go::GoParser;

const MAIN: &str = r#"package main

import (
	"encoding/json"
	"errors"
	str "strings"
	"path"
	"example.com/shop/ledger"
	. "fmt"
)

func run(data []byte, p string) error {
	var v map[string]int
	if err := json.Unmarshal(data, &v); errors.Is(err, ledger.ErrClosed) {
		return err
	}
	path := str.TrimSpace(p)
	Println(path)
	return ledger.Close()
}
"#;

const LEDGER: &str = r#"package ledger

import "errors"

var ErrClosed = errors.New("closed")

type Book struct{}

func (Book) Is(target error) bool { return false }
func (Book) Unmarshal(b []byte) error { return nil }
func TrimSpace(s string) string { return s }
func Close() error { return nil }
"#;

fn parsed(path: &str, source: &str) -> ParsedFile {
    mct_corpus::parse_source(&GoParser, path, "Go", source)
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
fn a_standard_package_call_is_external() {
    assert_eq!(
        calls(&parsed("cmd/main.go", MAIN)),
        [
            "Unmarshal external",
            "Is external",
            "TrimSpace external",
            "Println",
            "Close",
        ]
    );
}

#[test]
fn a_rebound_package_name_is_not_proven() {
    let source = r#"package main

import "errors"

func f(errors []error) bool {
	return errors.Is(nil, nil)
}
"#;
    assert_eq!(calls(&parsed("cmd/f.go", source)), ["Is"]);
}

#[test]
fn a_standard_call_never_resolves_to_a_repository_method() {
    let root = std::env::temp_dir().join(format!("mct-go-std-{}", std::process::id()));
    std::fs::create_dir_all(root.join("cmd")).unwrap();
    std::fs::create_dir_all(root.join("ledger")).unwrap();
    std::fs::write(root.join("cmd/main.go"), MAIN).unwrap();
    std::fs::write(root.join("ledger/ledger.go"), LEDGER).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(GoParser));
    let mut hits: Vec<String> = ["Is", "Unmarshal", "TrimSpace", "Close"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| {
            format!(
                "{} {}:{} {:?}",
                h.to_name, h.relative_path, h.line, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "Close cmd/main.go:19 Resolved",
            "Is cmd/main.go:14 External",
            "TrimSpace cmd/main.go:17 External",
            "Unmarshal cmd/main.go:14 External",
        ]
    );
}
