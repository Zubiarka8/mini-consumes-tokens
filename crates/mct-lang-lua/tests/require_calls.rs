//! A call through a module a literal `require` bound reaches that module's
//! file: `a.render(1)` after `local a = require("a")` is `a.lua`'s
//! `render`, never `b.lua`'s, whether the call goes through the module
//! table, a dotted module path or a local alias of one of its fields. A
//! table the file builds itself and a name no `require` bound prove nothing.
//!
//! The project is written to a temporary directory, so its clashing names
//! never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_lua::LuaParser;

const MODULE: &str = "local M = {}\nfunction M.render(x) return 'MODULE' end\nreturn M\n";

const MAIN: &str = "\
local a = require(\"a\")
local fmt = require(\"b\")
local deep = require(\"lib.c\")
local render = fmt.render
local own = {}
function own.render(x) return 'own' end

a.render(1)
fmt.render(2)
render(3)
deep.render(4)
own.render(5)
";

fn indexed() -> (Index, std::path::PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-lua-require-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("lib")).unwrap();
    for file in ["a.lua", "b.lua", "lib/c.lua"] {
        std::fs::write(root.join(file), MODULE).unwrap();
    }
    std::fs::write(root.join("main.lua"), MAIN).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(LuaParser));
    (index, root)
}

fn call(index: &Index, line: u32) -> RelationHit {
    index
        .find_references("render")
        .unwrap()
        .into_iter()
        .find(|h| h.kind == "calls" && h.relative_path == "main.lua" && h.line == line)
        .unwrap()
}

#[test]
fn a_call_through_a_required_module_reaches_its_file() {
    let (index, root) = indexed();
    for (line, file) in [(8, "a.lua"), (9, "b.lua"), (10, "b.lua"), (11, "lib/c.lua")] {
        let hit = call(&index, line);
        assert_eq!(hit.resolution, Resolution::Resolved, "line {line}: {hit:?}");
        assert_eq!(
            hit.candidates[0].relative_path, file,
            "line {line}: {hit:?}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_call_on_a_table_the_file_builds_is_not_a_module_call() {
    let (index, root) = indexed();
    let hit = call(&index, 12);
    assert!(
        hit.resolution != Resolution::Resolved || hit.candidates[0].relative_path == "main.lua",
        "{hit:?}"
    );
    std::fs::remove_dir_all(root).unwrap();
}
