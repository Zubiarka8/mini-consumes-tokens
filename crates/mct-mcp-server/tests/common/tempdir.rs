use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn tempdir(prefix: &str) -> PathBuf {
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
    let id = nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed));
    let dir = std::env::temp_dir().join(format!("{prefix}{id}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}
