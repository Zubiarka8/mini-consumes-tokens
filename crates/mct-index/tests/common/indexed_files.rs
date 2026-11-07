use std::path::PathBuf;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};

pub fn indexed(
    dir: PathBuf,
    registry: &LanguageRegistry,
    files: &[(&str, &str)],
) -> (PathBuf, Index) {
    for (path, contents) in files {
        std::fs::write(dir.join(path), contents).unwrap();
    }
    let mut index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
    index.reindex(registry, false).unwrap();
    (dir, index)
}
