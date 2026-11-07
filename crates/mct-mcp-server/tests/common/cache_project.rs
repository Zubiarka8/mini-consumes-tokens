use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

pub fn project(prefix: &str, tag: &str, billing: &str) -> PathBuf {
    // Unique per call: tests run in parallel and some share a tag.
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "{prefix}-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/billing.rs"), billing).unwrap();
    std::fs::write(
        dir.join("src/notify.rs"),
        "pub fn send_notification(user: &str) -> usize {\n    user.len()\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/other.rs"),
        "pub fn unrelated_helper() -> u32 {\n    42\n}\n",
    )
    .unwrap();
    dir.canonicalize().unwrap()
}
