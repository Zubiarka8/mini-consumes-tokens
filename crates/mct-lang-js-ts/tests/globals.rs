//! ECMAScript globals (`Math`, `JSON`, `parseInt`, `Map`…) through the real
//! parse -> SQLite -> query pipeline. A global that nothing in its file
//! binds — no declaration, import, parameter or assignment of that name, no
//! `eval` and no `with` — is the platform's, so `Math.max(…)` must never
//! resolve to a same-named method in another file. When the file does bind
//! the name, the usual resolution applies.
//!
//! The project is written to a temporary directory rather than checked in
//! under `tests/fixtures/`, so its deliberately clashing names never become
//! candidates in this repository's own index. Each claim below was checked by
//! running the same files with Node's type stripping.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_js_ts::JsTsParser;

const MONEY: &str = "\
export class Money {
  max(other: Money): Money {
    return other;
  }

  stringify(): string {
    return \"money\";
  }
}

export class Map {}

export function parseInt(text: string): number {
  return 0;
}
";

const USES: &str = "\
export function show(a: number, b: number): string {
  const top = Math.max(a, b);
  return JSON.stringify(top) + parseInt(\"1\");
}

export function build(): unknown {
  return new Map();
}
";

const MATHLIB: &str = "\
export const Math = {
  max(a: number, b: number): number {
    return a;
  },
};
";

const SHADOW_LOCAL: &str = "\
const Math = { max: (a: number, b: number): number => a };

export function pick(a: number, b: number): number {
  return Math.max(a, b);
}
";

const SHADOW_IMPORT: &str = "\
import { Math } from \"./mathlib\";

export function pick(a: number, b: number): number {
  return Math.max(a, b);
}
";

const SHADOW_PARAM: &str = "\
export function show(JSON: { stringify(value: unknown): string }): string {
  return JSON.stringify(1);
}
";

const SHADOW_DECL: &str = "\
function parseInt(text: string): number {
  return 0;
}

export function read(): number {
  return parseInt(\"1\");
}
";

const DYNAMIC: &str = "\
export function read(code: string): number {
  eval(code);
  return parseInt(\"1\");
}
";

/// A fresh temporary copy of the project above, indexed in memory.
fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-js-ts-globals-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, contents) in [
        ("money.ts", MONEY),
        ("uses.ts", USES),
        ("mathlib.ts", MATHLIB),
        ("shadow_local.ts", SHADOW_LOCAL),
        ("shadow_import.ts", SHADOW_IMPORT),
        ("shadow_param.ts", SHADOW_PARAM),
        ("shadow_decl.ts", SHADOW_DECL),
        ("dynamic.ts", DYNAMIC),
    ] {
        std::fs::write(root.join(name), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    (index, root)
}

/// The one `calls` relation named `to` in `file` on `line`.
fn call(index: &Index, file: &str, to: &str, line: u32) -> RelationHit {
    let hits: Vec<_> = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == file && h.kind == "calls" && h.line == line)
        .collect();
    assert_eq!(hits.len(), 1, "calls {to} at {file}:{line}: {hits:?}");
    hits.into_iter().next().unwrap()
}

#[test]
fn unbound_globals_are_external_not_a_same_named_definition_elsewhere() {
    let (index, root) = indexed();
    for (to, line) in [("max", 2), ("stringify", 3), ("parseInt", 3), ("Map", 7)] {
        let hit = call(&index, "uses.ts", to, line);
        assert_eq!(hit.resolution, Resolution::External, "{to}: {hit:?}");
    }
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_global_name_bound_in_the_file_is_never_external() {
    let (index, root) = indexed();
    for (file, to, line) in [
        ("shadow_local.ts", "max", 4),
        ("shadow_import.ts", "max", 4),
        ("shadow_param.ts", "stringify", 2),
        ("shadow_decl.ts", "parseInt", 6),
        // A direct `eval` can declare any name.
        ("dynamic.ts", "parseInt", 3),
    ] {
        let hit = call(&index, file, to, line);
        assert_ne!(hit.resolution, Resolution::External, "{file} {to}: {hit:?}");
    }
    std::fs::remove_dir_all(root).ok();
}
