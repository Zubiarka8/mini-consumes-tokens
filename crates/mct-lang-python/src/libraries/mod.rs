//! Library-pattern regression tests for the shared Python parser.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, LazyLock};

use mct_core::SymbolKind;
use mct_corpus::Corpus;

mod django;
mod flask;

fn corpus() -> &'static Corpus {
    static CORPUS: LazyLock<Corpus> =
        LazyLock::new(|| Corpus::load(Arc::new(crate::PythonParser), env!("CARGO_MANIFEST_DIR")));
    &CORPUS
}

fn lines(path: &str, name: &str, kind: SymbolKind) -> (u32, Option<u32>) {
    let s = corpus().symbol(path, name, kind);
    (s.location.line, s.location.end_line)
}

fn parent(path: &str, name: &str, kind: SymbolKind) -> Option<&'static str> {
    corpus().symbol(path, name, kind).parent.as_deref()
}
