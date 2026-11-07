//! The per-test scratch directory the integration tests build their toy
//! projects in.
//!
//! Each test file includes this file with `#[path]`, so every test binary
//! keeps its own counter, exactly as the per-file copies it replaces did.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// A fresh, empty, canonicalized directory under the system temp dir, named
/// `{prefix}-{pid}-{counter}` so parallel tests never share one.
pub fn scratch_dir(prefix: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}
