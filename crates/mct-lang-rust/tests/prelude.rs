//! Names from the standard prelude (`Ok`, `Some`, `String`, `Box`, the
//! `Debug`/`Default` derives…) and names imported from `std`, through the
//! real parse -> SQLite -> query pipeline. A bare prelude name that nothing
//! in its file declares, imports or could bring in through a glob is the
//! prelude's, so it must never resolve to a same-named enum variant in
//! another file. When the file does declare or import the name, the usual
//! resolution applies.
//!
//! The project is written to a temporary directory rather than checked in
//! under `tests/fixtures/`, so its deliberately clashing names (`Ok`, `Some`,
//! `Default` variants) never become candidates in this repository's own index.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-rust/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use mct_index::{Index, RelationHit, Resolution};
use mct_lang_rust::RustParser;

const KINDS: &str = "\
pub enum Outcome { Ok(u8), Err(u8) }
pub enum Severity { Debug, Info }
pub enum Strategy { Default, Other }
pub enum Category { String, Box, Eq, Option }
pub trait Display {}

pub fn pick() -> Strategy {
    Strategy::Default
}
";

const USES: &str = "\
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Plain {
    name: String,
    inner: Option<Box<u8>>,
}

pub struct Custom;

impl Default for Custom {
    fn default() -> Self {
        Custom
    }
}

pub fn ok_value() -> Result<u8, String> {
    Ok(1)
}

pub fn err_value() -> Result<u8, String> {
    Err(String::new())
}

pub fn some_value() -> Option<u8> {
    Some(1)
}

pub fn boxed() -> Box<u8> {
    Box::new(1)
}

mod tests {
    use super::*;

    fn in_test() -> Result<u8, String> {
        Ok(ok_value()?)
    }
}
";

const IMPORTED: &str = "\
use crate::kinds::Outcome::Ok;

pub fn explicit() -> crate::kinds::Outcome {
    Ok(1)
}
";

const GLOB: &str = "\
use crate::kinds::Outcome::*;

pub fn via_glob() -> crate::kinds::Outcome {
    Ok(1)
}
";

const SCOPED_GLOB: &str = "\
pub fn scoped() -> crate::kinds::Outcome {
    use crate::kinds::Outcome::*;
    Ok(1)
}

pub fn outside() -> Result<u8, u8> {
    Ok(2)
}
";

const SCOPED_IMPORT: &str = "\
pub fn inside() -> crate::kinds::Outcome {
    use crate::kinds::Outcome::Ok;
    Ok(1)
}

pub fn outside() -> Result<u8, u8> {
    Ok(2)
}
";

const LOCAL: &str = "\
pub struct Some(pub u8);

pub fn local_some() -> Some {
    Some(1)
}
";

const SHADOW: &str = "\
pub fn shadowed() -> u8 {
    let drop = |x: u8| x;
    drop(1)
}
";

const STD_IMPORT: &str = "\
use std::fmt;
use std::fmt::Debug;

pub struct Shown;

impl fmt::Display for Shown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        Ok(())
    }
}

impl Debug for Shown {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        Ok(())
    }
}
";

const CONVERT: &str = "\
pub struct Meters(pub u32);

impl From<u32> for Meters {
    fn from(value: u32) -> Self {
        Meters(value)
    }
}
";

/// A fresh temporary copy of the project above, indexed in memory.
fn indexed() -> (Index, PathBuf) {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mct-lang-rust-prelude-{}-{unique}",
        std::process::id()
    ));
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    for (name, contents) in [
        ("kinds.rs", KINDS),
        ("uses.rs", USES),
        ("imported.rs", IMPORTED),
        ("glob.rs", GLOB),
        ("scoped_glob.rs", SCOPED_GLOB),
        ("scoped_import.rs", SCOPED_IMPORT),
        ("local.rs", LOCAL),
        ("shadow.rs", SHADOW),
        ("std_import.rs", STD_IMPORT),
        ("convert.rs", CONVERT),
    ] {
        std::fs::write(src.join(name), contents).unwrap();
    }
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(RustParser));
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

/// The file the relation resolved to, if it resolved.
fn target_file(index: &Index, hit: &RelationHit) -> Option<String> {
    let id = hit.target_id?;
    index
        .symbols_by_ids(&[id])
        .unwrap()
        .into_iter()
        .next()
        .map(|s| s.relative_path)
}

#[test]
fn bare_prelude_names_are_external_not_a_variant_elsewhere() {
    let (index, root) = indexed();
    let file = "src/uses.rs";
    for (kind, to, line) in [
        ("references", "Debug", 1),
        ("references", "Default", 1),
        ("references", "Eq", 1),
        ("references", "String", 3),
        ("references", "Option", 4),
        ("references", "Box", 4),
        ("calls", "Ok", 16),
        ("calls", "Err", 20),
        ("calls", "Some", 24),
        // `Box::new`: the path's type is the prelude's `Box`.
        ("references", "Box", 28),
    ] {
        let hit = hit(&index, file, kind, to, line);
        assert_eq!(
            hit.resolution,
            Resolution::External,
            "{kind} {to} at {file}:{line} -> {:?}",
            target_file(&index, &hit)
        );
    }
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_trait_impl_names_the_trait_not_a_same_named_variant() {
    let (index, root) = indexed();
    // `impl Default for Custom`: the prelude trait, never `Strategy::Default`.
    let default = hit(&index, "src/uses.rs", "implements", "Default", 9);
    assert_eq!(default.resolution, Resolution::External);
    // `impl From<u32> for Meters` names `From`, without its generic argument.
    let from = hit(&index, "src/convert.rs", "implements", "From", 3);
    assert_eq!(from.resolution, Resolution::External);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_super_glob_in_an_inline_module_keeps_the_prelude() {
    let (index, root) = indexed();
    // `mod tests { use super::*; }` sees only this file's items and imports,
    // none of which is named `Ok`.
    let hit = hit(&index, "src/uses.rs", "calls", "Ok", 35);
    assert_eq!(hit.resolution, Resolution::External);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn an_explicit_import_of_a_variant_is_resolved_to_it() {
    let (index, root) = indexed();
    let hit = hit(&index, "src/imported.rs", "calls", "Ok", 4);
    assert_eq!(hit.resolution, Resolution::Resolved);
    assert_eq!(target_file(&index, &hit).as_deref(), Some("src/kinds.rs"));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_glob_import_from_elsewhere_is_never_proof_of_the_prelude() {
    let (index, root) = indexed();
    // `use crate::kinds::Outcome::*` may bring an `Ok` in: not external.
    let hit = hit(&index, "src/glob.rs", "calls", "Ok", 4);
    assert_ne!(hit.resolution, Resolution::External);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_glob_in_a_function_reaches_only_that_function() {
    let (index, root) = indexed();
    let inside = hit(&index, "src/scoped_glob.rs", "calls", "Ok", 3);
    assert_ne!(inside.resolution, Resolution::External);
    let outside = hit(&index, "src/scoped_glob.rs", "calls", "Ok", 7);
    assert_eq!(outside.resolution, Resolution::External);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn an_import_in_a_function_shadows_the_prelude_only_there() {
    let (index, root) = indexed();
    let inside = hit(&index, "src/scoped_import.rs", "calls", "Ok", 3);
    assert_eq!(inside.resolution, Resolution::Resolved);
    assert_eq!(
        target_file(&index, &inside).as_deref(),
        Some("src/kinds.rs")
    );
    let outside = hit(&index, "src/scoped_import.rs", "calls", "Ok", 7);
    assert_eq!(outside.resolution, Resolution::External);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_local_definition_shadows_the_prelude() {
    let (index, root) = indexed();
    let some = hit(&index, "src/local.rs", "calls", "Some", 4);
    assert_eq!(some.resolution, Resolution::Resolved);
    assert_eq!(target_file(&index, &some).as_deref(), Some("src/local.rs"));
    // A local closure named `drop` is called, not `std::mem::drop`.
    let drop = hit(&index, "src/shadow.rs", "calls", "drop", 3);
    assert_ne!(drop.resolution, Resolution::External);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn names_imported_from_std_are_external() {
    let (index, root) = indexed();
    let file = "src/std_import.rs";
    // `impl fmt::Display`: `fmt` comes from `use std::fmt`, so this is not
    // the `Display` trait declared in `kinds.rs`.
    let display = hit(&index, file, "implements", "Display", 6);
    assert_eq!(display.resolution, Resolution::External);
    // `impl Debug` with `use std::fmt::Debug`.
    let debug = hit(&index, file, "implements", "Debug", 13);
    assert_eq!(debug.resolution, Resolution::External);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn a_qualified_variant_still_resolves_to_its_enum() {
    let (index, root) = indexed();
    let hit = hit(&index, "src/kinds.rs", "references", "Default", 8);
    assert_eq!(hit.resolution, Resolution::Resolved);
    assert_eq!(target_file(&index, &hit).as_deref(), Some("src/kinds.rs"));
    std::fs::remove_dir_all(root).ok();
}
