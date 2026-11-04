//! The unique-enough number the `mct-index` integration tests use to name
//! their per-test temporary directories.
//!
//! Each test file includes this file with `#[path]`, so every test binary
//! keeps its own counter, exactly as the per-file copies it replaces did.

/// Wall-clock nanoseconds plus a per-process counter, so two directories
/// created in the same nanosecond by parallel tests still get different names.
pub fn uuid_like() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed))
}
