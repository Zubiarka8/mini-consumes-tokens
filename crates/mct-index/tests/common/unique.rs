//! The unique-enough number the `mct-index` integration tests use to name
//! their per-test temporary directories.
//!
//! Each test file includes this file with `#[path]`, so every test binary
//! keeps its own counter, exactly as the per-file copies it replaces did.

/// The process's first clock reading plus a per-process counter, so two
/// parallel tests in one binary never get the same name.
pub fn uuid_like() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    // The clock is read once per process: re-reading it per call let a
    // later call's earlier reading plus a higher count land on the same id.
    static BASE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let nanos = *BASE.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    });
    nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}
