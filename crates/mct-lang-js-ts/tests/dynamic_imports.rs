//! A name bound to a dynamically imported module carries that module as
//! evidence, as a static import does, through the real parse -> SQLite ->
//! query pipeline: `const m = await import("./lazy"); m.load()` and
//! `const { load } = await import("./lazy"); load()` are `lazy`'s `load`,
//! never `eager`'s, and so is the module parameter of
//! `import("./lazy").then((m) => m.load())`. A promise kept without
//! `await`, a renamed destructuring and a name the file binds again get no
//! module evidence. Each case was run with Node 24 type stripping: the
//! promise form and the awaited form load the same module namespace.
//!
//! The project is written to a temporary directory, so its clashing names
//! never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_js_ts::JsTsParser;

const FILES: &[(&str, &str)] = &[
    (
        "src/lazy.ts",
        "export function load(): number {\n  return 1;\n}\nexport function save(): number {\n  return 2;\n}\n",
    ),
    (
        "src/eager.ts",
        "export function load(): number {\n  return 3;\n}\nexport function save(): number {\n  return 4;\n}\n",
    ),
    (
        "src/app.ts",
        "\
export async function viaNamespace(): Promise<number> {
  const m = await import(\"./lazy.ts\");
  return m.load();
}

export async function viaDestructuring(): Promise<number> {
  const { save } = await import(\"./lazy.ts\");
  return save();
}

export function viaThen(): Promise<number> {
  return import(\"./lazy.ts\").then((mod) => mod.load(), (err) => err.save());
}

export function viaPromise(): Promise<number> {
  const pending = import(\"./lazy.ts\");
  return pending.then((x) => x.save());
}
",
    ),
];

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-js-ts-dynamic-imports-{}-{unique}",
        std::process::id()
    ));
    for (path, source) in FILES {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, source).unwrap();
    }
    let index = mct_corpus::open_indexed(&root, Arc::new(JsTsParser), FILES.len(), "src/*.ts");
    (index, root)
}

fn call_from(index: &Index, name: &str, line: u32) -> RelationHit {
    let hits: Vec<_> = index
        .find_references(name)
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == "src/app.ts" && h.line == line && h.kind == "calls")
        .collect();
    assert_eq!(hits.len(), 1, "{name} at {line}: {hits:?}");
    hits.into_iter().next().unwrap()
}

fn target_path(index: &Index, hit: &RelationHit) -> String {
    assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    let target = index.symbols_by_ids(&[hit.target_id.unwrap()]).unwrap();
    target[0].relative_path.clone()
}

#[test]
fn an_awaited_import_binds_its_module() {
    let (index, root) = indexed();
    assert_eq!(
        target_path(&index, &call_from(&index, "load", 3)),
        "src/lazy.ts"
    );
    assert_eq!(
        target_path(&index, &call_from(&index, "save", 8)),
        "src/lazy.ts"
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_then_callback_parameter_is_the_module() {
    let (index, root) = indexed();
    assert_eq!(
        target_path(&index, &call_from(&index, "load", 12)),
        "src/lazy.ts"
    );
    // The rejection handler's parameter is an error, not the module.
    assert_ne!(
        call_from(&index, "save", 12).resolution,
        Resolution::Resolved
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_promise_kept_without_await_proves_nothing() {
    let (index, root) = indexed();
    assert_ne!(
        call_from(&index, "save", 17).resolution,
        Resolution::Resolved
    );
    std::fs::remove_dir_all(root).ok();
}
