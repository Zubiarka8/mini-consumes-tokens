//! Leaving the process while startup indexing is still running.
//!
//! The index transaction rolls back on its own, but the post-reindex hook
//! publishes the Obsidian vault under a writer lock that only its `Drop`
//! removes. Exiting in the middle of that publication would leave the lock
//! behind and every later publication would fail. The hook holds a
//! [`PublishGuard`] while it publishes, so the exit takes the guard first: it
//! waits for exactly as long as the publication needs, with no timeout to
//! guess, and once it holds the guard no new publication can start.

use mct_obsidian::PublishGuard;

/// Runs `exit` once no publication is in progress. `exit` is called while the
/// guard is still held, so a publication cannot begin between the two.
pub fn exit_when_not_publishing(guard: &PublishGuard, exit: impl FnOnce()) {
    let _idle = guard.lock().unwrap_or_else(|e| e.into_inner());
    exit();
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn a_publisher_that_panicked_does_not_stop_the_exit() {
        let guard: PublishGuard = Arc::default();
        let poisoner = {
            let guard = guard.clone();
            std::thread::spawn(move || {
                let _publishing = guard.lock().unwrap();
                panic!("publication failed midway");
            })
        };
        assert!(poisoner.join().is_err());
        assert!(guard.is_poisoned());
        let mut exited = false;
        exit_when_not_publishing(&guard, || exited = true);
        assert!(
            exited,
            "a poisoned guard must not keep the process from exiting"
        );
    }

    #[test]
    fn exit_waits_for_a_publication_that_is_in_progress() {
        let dir = std::env::temp_dir().join(format!("mct-shutdown-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lock = dir.join("vault.lock");
        let guard: PublishGuard = Arc::default();
        let (started_tx, started_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel::<()>();

        // Stand-in for the hook: holds the guard and the lock file while it works.
        let publisher = {
            let (guard, lock) = (guard.clone(), lock.clone());
            std::thread::spawn(move || {
                let _publishing = guard.lock().unwrap();
                std::fs::write(&lock, "writer").unwrap();
                started_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
                std::fs::remove_file(&lock).unwrap();
            })
        };
        started_rx.recv().unwrap();

        let (exit_tx, exit_rx) = mpsc::channel();
        let exiter = {
            let (guard, lock) = (guard.clone(), lock.clone());
            std::thread::spawn(move || {
                exit_when_not_publishing(&guard, || {
                    exit_tx.send(lock.exists()).unwrap();
                });
            })
        };
        // The publication is blocked on `finish_rx`, so the exit cannot have run.
        assert!(exit_rx.recv_timeout(Duration::from_millis(300)).is_err());
        finish_tx.send(()).unwrap();
        publisher.join().unwrap();
        let lock_left_behind = exit_rx.recv_timeout(Duration::from_secs(20)).unwrap();
        exiter.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            !lock_left_behind,
            "exit ran with the writer lock still present"
        );
    }
}
