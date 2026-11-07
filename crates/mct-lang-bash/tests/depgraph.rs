//! File-level `Imports` evidence: `source`/`.` of a `.sh` file names the
//! `Module` of that file, pinned to its path when the source proves it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_index::{Index, Resolution};
use mct_lang_bash::BashParser;

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/depgraph-fixture");
    mct_corpus::open_fixture_indexed(&root, Arc::new(BashParser))
}

fn import_hits(index: &Index, to: &str) -> Vec<(Resolution, Option<String>)> {
    mct_corpus::import_hits_from(index, "bin/run.sh", to)
}

fn resolved(file: &str) -> (Resolution, Option<String>) {
    (Resolution::Resolved, Some(file.to_string()))
}

#[test]
fn script_relative_and_literal_sources_are_pinned_to_their_file() {
    let index = open_indexed();
    assert!(import_hits(&index, "log").contains(&resolved("lib/log.sh")));
    // `bin/helpers.sh` and `lib/helpers.sh` share a stem; the path decides.
    assert!(import_hits(&index, "helpers").contains(&resolved("bin/helpers.sh")));
}

#[test]
fn a_source_through_a_variable_proves_only_the_file_name() {
    let index = open_indexed();
    // `$LIB_DIR/fs.sh`: the stem is unique in the repository.
    assert!(import_hits(&index, "fs").contains(&resolved("lib/fs.sh")));
}

#[test]
fn absolute_home_and_non_script_sources_add_no_file_evidence() {
    let index = open_indexed();
    assert!(import_hits(&index, "system").is_empty());
    assert!(import_hits(&index, ".bashrc")
        .iter()
        .all(|(r, _)| *r != Resolution::Resolved));
}
