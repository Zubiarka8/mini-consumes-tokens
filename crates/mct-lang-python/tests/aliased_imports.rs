//! `from m import f as g` binds the local name `g` to the `f` that module
//! `m` defines, so a call `g()`, a decorator `@g`, a base class `g` and a
//! value use of `g` name `f`, through the real parse -> SQLite -> query
//! pipeline. The import itself imports `f`: a package `__init__.py` that
//! re-exports `from .core import other as public_other` keeps `other` alive.
//! A name the file binds again after the import (a `def g`) is this file's,
//! so it keeps its own name.
//!
//! The project is written to a temporary directory rather than checked in
//! under `tests/fixtures/`, so its names never become candidates in this
//! repository's own index. Each claim was checked by running the same files
//! with `python3` from the project root.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{Index, Resolution};
use mct_lang_python::PythonParser;

const FILES: &[(&str, &str)] = &[
    (
        "pkg/__init__.py",
        "from .core import compute, other as public_other\n",
    ),
    (
        "pkg/core.py",
        "\
def compute():
    return 1


def other():
    return 2


def third():
    return 3


def wrap(f):
    return f


class Base:
    pass
",
    ),
    (
        "main.py",
        "\
from pkg.core import third as t, wrap as w, Base as B
from pkg.core import compute as c


@w
def run():
    return t() + len([c])


class Child(B):
    pass
",
    ),
    (
        "shadow.py",
        "\
from pkg.core import third as t


def t():
    return 0


def run():
    return t()
",
    ),
];

fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-python-aliased-imports-{}-{unique}",
        std::process::id()
    ));
    for (path, contents) in FILES {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PythonParser));
    (index, root)
}

/// `kind name -> file` for each relation from `file` that resolves.
fn resolved_from(index: &Index, file: &str, names: &[&str]) -> Vec<String> {
    let mut rows = Vec::new();
    for name in names {
        for hit in index.find_references(name).unwrap() {
            if hit.relative_path != file || hit.resolution != Resolution::Resolved {
                continue;
            }
            let Some(id) = hit.target_id else { continue };
            let target = index.symbols_by_ids(&[id]).unwrap();
            let path = target.first().map(|s| s.relative_path.clone());
            rows.push(format!(
                "{} {} -> {}",
                hit.kind,
                hit.to_name,
                path.unwrap_or_default()
            ));
        }
    }
    rows.sort();
    rows
}

#[test]
fn an_aliased_import_names_the_original_definition() {
    let (index, root) = indexed();
    let rows = resolved_from(
        &index,
        "main.py",
        &["third", "wrap", "Base", "compute", "t", "w", "B", "c"],
    );
    for row in [
        "calls third -> pkg/core.py",
        "references wrap -> pkg/core.py",
        "references compute -> pkg/core.py",
        "imports third -> pkg/core.py",
        "imports wrap -> pkg/core.py",
        "imports Base -> pkg/core.py",
        "imports compute -> pkg/core.py",
    ] {
        assert!(rows.iter().any(|r| r == row), "{row} not in {rows:#?}");
    }
    assert!(
        rows.iter()
            .any(|r| r.ends_with(" Base -> pkg/core.py") && !r.starts_with("imports")),
        "the base class `B` names `Base`: {rows:#?}"
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_package_re_export_under_another_name_keeps_the_original_alive() {
    let (index, root) = indexed();
    let rows = resolved_from(&index, "pkg/__init__.py", &["other", "public_other"]);
    assert_eq!(rows, ["imports other -> pkg/core.py"]);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_name_bound_again_after_an_aliased_import_keeps_its_own_name() {
    let (index, root) = indexed();
    let calls: Vec<_> = index
        .find_references("third")
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == "shadow.py" && h.kind == "calls")
        .collect();
    assert!(calls.is_empty(), "{calls:?}");
    std::fs::remove_dir_all(root).ok();
}
