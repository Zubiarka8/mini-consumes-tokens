#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_lua::LuaParser;

pub fn open_indexed(fixture: &str) -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{fixture}"));
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(LuaParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    index
}
