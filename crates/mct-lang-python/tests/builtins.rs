//! Python builtins (`open`, `len`, `max`, `Exception`…) through the real
//! parse -> SQLite -> query pipeline. A bare builtin name that nothing in its
//! file binds — no `def`, `class`, import, parameter, assignment or loop
//! target of that name, no `from m import *` and no `globals()`/`exec` that
//! could bind it — is looked up in `builtins` by Python, so it must never
//! resolve to a same-named method or function in another file. When the file
//! does bind the name, the usual resolution applies.
//!
//! The project is written to a temporary directory rather than checked in
//! under `tests/fixtures/`, so its deliberately clashing names never become
//! candidates in this repository's own index. Each claim below was checked by
//! running the same files with `python -I`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_python::PythonParser;

const LOAN: &str = "\
class Loan:
    def open(self):
        return True

    def len(self):
        return 0


def max(*values):
    return 0
";

const USES: &str = "\
def summarize(path):
    with open(path) as handle:
        rows = handle.read().split()
    return len(rows)


def biggest(values):
    return max(values)


class Failure(Exception):
    pass


def fail():
    raise ValueError(\"x\")
";

const DECLARED: &str = "\
def len(values):
    return 0


def count(values):
    return len(values)
";

const STAR: &str = "\
from loan import *


def top(values):
    return max(values)
";

const IMPORTED: &str = "\
from loan import max


def top(values):
    return max(values)
";

const PARAM: &str = "\
def apply(len, values):
    return len(values)
";

const ASSIGNED: &str = "\
import gzip

open = gzip.open


def read(path):
    return open(path)
";

const LOOP: &str = "\
def pick(options, values):
    for max in options:
        return max(values)
";

const DYNAMIC: &str = "\
globals()[\"len\"] = lambda values: 0


def count(values):
    return len(values)
";

/// A fresh temporary copy of the project above, indexed in memory.
fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-python-builtins-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, contents) in [
        ("loan.py", LOAN),
        ("uses.py", USES),
        ("declared.py", DECLARED),
        ("star.py", STAR),
        ("imported.py", IMPORTED),
        ("param.py", PARAM),
        ("assigned.py", ASSIGNED),
        ("loop.py", LOOP),
        ("dynamic.py", DYNAMIC),
    ] {
        std::fs::write(root.join(name), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PythonParser));
    (index, root)
}

/// The one relation of `kind` named `to` in `file` on `line`.
fn hit(index: &Index, file: &str, kind: &str, to: &str, line: u32) -> RelationHit {
    let hits: Vec<_> = index
        .find_references(to)
        .unwrap()
        .into_iter()
        .filter(|h| h.relative_path == file && h.kind == kind && h.line == line)
        .collect();
    assert_eq!(hits.len(), 1, "{kind} {to} at {file}:{line}: {hits:?}");
    hits.into_iter().next().unwrap()
}

#[test]
fn unbound_builtins_are_external_not_a_same_named_definition_elsewhere() {
    let (index, root) = indexed();
    for (kind, to, line) in [
        ("calls", "open", 2),
        ("calls", "len", 4),
        ("calls", "max", 8),
        ("extends", "Exception", 11),
        ("calls", "ValueError", 16),
    ] {
        let hit = hit(&index, "uses.py", kind, to, line);
        assert_eq!(hit.resolution, Resolution::External, "{kind} {to}: {hit:?}");
    }
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_builtin_name_bound_in_the_file_is_never_external() {
    let (index, root) = indexed();
    for (file, to, line) in [
        // `def len` in the same module.
        ("declared.py", "len", 6),
        // `from loan import *` may bind `max`.
        ("star.py", "max", 5),
        ("imported.py", "max", 5),
        // A parameter, a module-level assignment and a loop target.
        ("param.py", "len", 2),
        ("assigned.py", "open", 7),
        ("loop.py", "max", 3),
        // `globals()` can bind any name at run time.
        ("dynamic.py", "len", 5),
    ] {
        let hit = hit(&index, file, "calls", to, line);
        assert_ne!(hit.resolution, Resolution::External, "{file} {to}: {hit:?}");
    }
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_same_module_definition_still_resolves_to_itself() {
    let (index, root) = indexed();
    let hit = hit(&index, "declared.py", "calls", "len", 6);
    assert_eq!(hit.resolution, Resolution::Resolved);
    std::fs::remove_dir_all(root).ok();
}
