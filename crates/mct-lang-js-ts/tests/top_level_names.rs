//! Issue #395: a name written with no receiver (`f()`, `new X()`, `<X />`,
//! a type `X`, `import { f }`) names a top-level binding, never a member of
//! a class, an interface or an enum. A member access still sees members.
//!
//! The project is written to a temporary directory, so its names never
//! become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_core::{LanguageParser, SourceFile};
use mct_index::{Index, Resolution};
use mct_lang_js_ts::JsTsParser;

fn indexed(files: &[(&str, &str)]) -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-js-ts-top-level-{}-{unique}",
        std::process::id()
    ));
    for (name, contents) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    (index, root)
}

/// `(resolution, candidate "path:line")` of every `name` hit from `file`
/// at `line`.
fn hits(index: &Index, name: &str, file: &str, line: u32) -> Vec<(Resolution, Vec<String>)> {
    index
        .find_references(name)
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == file && h.line == line)
        .map(|h| {
            let mut c: Vec<String> = h
                .candidates
                .iter()
                .map(|c| format!("{}:{}", c.relative_path, c.line))
                .collect();
            c.sort();
            (h.resolution, c)
        })
        .collect()
}

const MODELS: &str = "\
export class Money { format() { return ''; } }
export class TableState { loading = false; }
export interface Bridge { stateLabel(): string; }
export enum State { Paid, Open }
";

#[test]
fn a_bare_name_resolves_to_the_top_level_binding_only() {
    let (index, root) = indexed(&[
        ("models.ts", MODELS),
        (
            "util.ts",
            "export function format() { return ''; }\n\
             export function loading() { return 1; }\n\
             export function stateLabel() { return ''; }\n\
             export type Paid = { at: number };\n",
        ),
        (
            "app.ts",
            "import { loading } from \"./util\";\n\
             format();\n\
             stateLabel();\n\
             let p: Paid;\n\
             loading();\n",
        ),
    ]);
    let one = |at: &str| vec![(Resolution::Resolved, vec![at.to_string()])];
    assert_eq!(hits(&index, "format", "app.ts", 2), one("util.ts:1"));
    assert_eq!(hits(&index, "stateLabel", "app.ts", 3), one("util.ts:3"));
    assert_eq!(hits(&index, "Paid", "app.ts", 4), one("util.ts:4"));
    assert_eq!(hits(&index, "loading", "app.ts", 5), one("util.ts:2"));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_member_access_still_sees_members() {
    let (index, root) = indexed(&[
        ("models.ts", MODELS),
        (
            "app.ts",
            "function show(m) { return m.format(); }\nlet s = State.Paid;\n",
        ),
    ]);
    let format = hits(&index, "format", "app.ts", 1);
    assert_eq!(format.len(), 1, "{format:?}");
    assert!(
        format[0].1.contains(&"models.ts:1".to_string()),
        "{format:?}"
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn the_parser_marks_only_names_without_a_receiver() {
    let parsed = JsTsParser
        .parse(&SourceFile {
            relative_path: "a.tsx".to_string(),
            contents: "import { f } from \"./m\";\n\
                       f(); o.g(); new Repo(); new ns.Repo();\n\
                       const x = <Button />; const y = <UI.Card />;\n\
                       let t: Paid; let u: State.Open;\n\
                       if (v instanceof Marker) {}\n"
                .to_string(),
        })
        .unwrap();
    let mut marked: Vec<(&str, bool)> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind != mct_core::RelationKind::Imports || r.to_name == "f")
        .map(|(i, r)| {
            let top = parsed
                .relation_targets
                .iter()
                .any(|t| t.relation == i && t.top_level);
            (r.to_name.as_str(), top)
        })
        .collect();
    marked.sort();
    marked.dedup();
    let expect = |name: &str| marked.iter().find(|(n, _)| *n == name).map(|m| m.1);
    assert_eq!(expect("f"), Some(true), "{marked:?}");
    assert_eq!(expect("g"), Some(false), "{marked:?}");
    assert_eq!(expect("Button"), Some(true), "{marked:?}");
    assert_eq!(expect("UI.Card"), Some(false), "{marked:?}");
    assert_eq!(expect("Paid"), Some(true), "{marked:?}");
    assert_eq!(expect("Open"), Some(false), "{marked:?}");
    assert_eq!(expect("Marker"), Some(true), "{marked:?}");
    assert!(marked.contains(&("Repo", true)), "{marked:?}");
    assert!(marked.contains(&("Repo", false)), "{marked:?}");
}
