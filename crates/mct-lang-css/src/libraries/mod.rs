//! Library-pattern regression tests for the shared CSS parser.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, LazyLock};

use mct_core::SymbolKind;
use mct_corpus::Corpus;

mod bootstrap;
mod tailwind;

fn corpus() -> &'static Corpus {
    static CORPUS: LazyLock<Corpus> =
        LazyLock::new(|| Corpus::load(Arc::new(crate::CssParser), env!("CARGO_MANIFEST_DIR")));
    &CORPUS
}

/// Every `(line, end_line)` of the `Rule` symbols named `name` in `path`,
/// in file order — a selector may be written many times in one file.
fn rules(path: &str, name: &str) -> Vec<(u32, u32)> {
    mct_corpus::symbol_ranges(corpus(), path, name, SymbolKind::Rule)
}
