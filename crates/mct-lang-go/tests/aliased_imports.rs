//! An aliased repository import (`db "example.com/app/database"`) proves
//! where the package lives: under the import path's last directory,
//! whatever its `package` clause says. So `db.Open()` and a declared type
//! `db.Conn` reach the declaration under `database/`, never a same-named
//! one in another directory. A blank (`_`) or dot (`.`) import binds no
//! name, a local named like the alias is a value, and a standard package
//! keeps its own evidence (external). A struct used only through the
//! alias is no dead-code candidate.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_lang_go::GoParser;

const RUN: &str = r#"package shop

import (
	db "example.com/app/database"
	j "encoding/json"
	_ "example.com/app/drivers"
	. "example.com/app/helpers"
)

func run(c db.Conn) {
	db.Open()
	j.Marshal(c)
	Assist()
}

func other() {
	db := local()
	db.Open()
}
"#;

fn hits(index: &mct_index::Index, names: &[&str]) -> Vec<String> {
    let mut hits: Vec<String> = names
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.relative_path == "shop/run.go")
        .map(|h| {
            let targets: Vec<String> = h
                .candidates
                .iter()
                .map(|c| c.relative_path.clone())
                .collect();
            format!(
                "{} {}:{} {:?} {targets:?}",
                h.kind, h.to_name, h.line, h.resolution
            )
        })
        .collect();
    hits.sort();
    hits
}

#[test]
fn an_aliased_import_reaches_the_package_under_its_directory() {
    let root = std::env::temp_dir().join(format!("mct-go-aliased-{}", std::process::id()));
    for dir in ["shop", "database", "other"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join("shop/run.go"), RUN).unwrap();
    // The package clause need not match the directory.
    std::fs::write(
        root.join("database/conn.go"),
        "package sqlstore\n\ntype Conn struct{}\n\nfunc Open() {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("other/conn.go"),
        "package other\n\ntype Conn struct{}\n\nfunc Open() {}\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(GoParser));
    let hits = hits(&index, &["Open", "Conn"]);
    let dead: Vec<String> = mct_index::find_dead_code_candidates(&index, None, None)
        .unwrap()
        .into_iter()
        .map(|d| format!("{} {}", d.relative_path, d.name))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert!(dead.contains(&"other/conn.go Conn".to_string()), "{dead:?}");
    assert!(
        !dead.contains(&"database/conn.go Conn".to_string()),
        "{dead:?}"
    );
    assert_eq!(
        hits,
        [
            "calls Open:11 Resolved [\"database/conn.go\"]",
            // A local `db` is a value: its `Open` is a method, and none is.
            "calls Open:18 Unresolved []",
            "references Conn:10 Resolved [\"database/conn.go\"]",
        ]
    );
}
